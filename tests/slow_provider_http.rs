//! Regression tests for the qwen-image "connection cut at ~60s" defect.
//!
//! Three consecutive real runs died at ~61s wall clock with a bare
//! `error sending request for url (…/multimodal-generation/generation)` even though the
//! provider asked for a 180s per-request timeout and the same POST answered in 52-60s
//! under curl. The cause is *connection-level*: `reqwest::Client::new()` leaves pool idle
//! retirement on and sends no TCP keepalive, so a render that goes quiet for a minute can
//! be dropped beneath the per-request deadline.
//!
//! These tests run against a local stub server, so they cost nothing and need no key:
//!
//! 1. The hardened client survives a response that arrives long after the client has gone
//!    quiet, and its own deadline is still enforced.
//! 2. The qwen-image provider's async task mode submits, polls, and downloads — so a long
//!    render never depends on one held-open connection at all.
//!
//! The slow-response delay is configurable via `MEDIA_TEST_SLOW_SECS` (default 3) to keep
//! the suite fast; `slow_response_past_a_real_gateway_timeout` reproduces the real ~90s
//! shape and is `#[ignore]`d by default. Run it with:
//!
//! ```sh
//! cargo test --test slow_provider_http -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use media_tool::providers::http;
use media_tool::providers::qwen_image::QwenImageProvider;
use media_tool::providers::{GenerationOptions, MediaProvider};
use media_tool::schema::AudioKind;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// ---------------------------------------------------------------------------
// Stub server helpers
// ---------------------------------------------------------------------------

/// Read an HTTP/1.1 request (request line, headers, and any `Content-Length` body).
async fn read_request(stream: &mut TcpStream) -> String {
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = stream.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&raw).to_string();
        if let Some(head_end) = text.find("\r\n\r\n") {
            let head = &text[..head_end];
            let content_len = head
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    (k.trim().eq_ignore_ascii_case("content-length"))
                        .then(|| v.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if raw.len() >= head_end + 4 + content_len {
                break;
            }
        }
    }
    String::from_utf8_lossy(&raw).to_string()
}

async fn write_json(stream: &mut TcpStream, body: &str) {
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
}

/// A server that accepts one connection, waits `delay`, then answers with `body`.
///
/// This is the shape that broke: the request is fully sent, then nothing crosses the wire
/// in either direction until the render finishes.
async fn spawn_slow_server(delay: Duration, body: &'static str) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let _ = read_request(&mut stream).await;
            tokio::time::sleep(delay).await;
            write_json(&mut stream, body).await;
        }
    });
    addr
}

fn slow_secs() -> u64 {
    std::env::var("MEDIA_TEST_SLOW_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(3)
}

// ---------------------------------------------------------------------------
// 1. Client-level cutoffs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hardened_client_survives_a_silent_wait_for_the_response() {
    let delay = Duration::from_secs(slow_secs());
    let addr = spawn_slow_server(delay, r#"{"ok":true}"#).await;

    let client = http::client_with_timeout(Duration::from_secs(300));
    let started = Instant::now();
    let resp = client
        .post(format!("http://{addr}/generate"))
        .json(&serde_json::json!({"model": "qwen-image-3.0"}))
        .timeout(Duration::from_secs(300))
        .send()
        .await;

    let resp = resp.expect("slow response must not be cut below the request deadline");
    assert!(resp.status().is_success());
    assert!(
        started.elapsed() >= delay,
        "server answered before it should have"
    );
}

#[tokio::test]
async fn hardened_client_still_enforces_its_own_deadline() {
    let delay = Duration::from_secs(slow_secs() * 4);
    let addr = spawn_slow_server(delay, r#"{"ok":true}"#).await;

    let client = http::client_with_timeout(Duration::from_secs(1));
    let err = client
        .get(format!("http://{addr}/slow"))
        .send()
        .await
        .expect_err("a 1s client timeout must fire on a much slower server");
    assert!(err.is_timeout(), "expected a timeout error, got: {err}");
}

/// The real-world shape: a render slower than the ~60s cut that was killing runs.
/// Ignored by default because it takes about 90s.
#[tokio::test]
#[ignore = "takes ~90s; run with --ignored to reproduce the original ~60s cut"]
async fn slow_response_past_a_real_gateway_timeout() {
    let delay = Duration::from_secs(90);
    let addr = spawn_slow_server(delay, r#"{"ok":true}"#).await;

    let client = http::client_with_timeout(Duration::from_secs(300));
    let started = Instant::now();
    let resp = client
        .post(format!("http://{addr}/generate"))
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .expect("a 90s render must not be cut at ~60s");
    assert!(resp.status().is_success());
    assert!(started.elapsed() >= Duration::from_secs(89));
}

// ---------------------------------------------------------------------------
// 2. qwen-image async task mode
// ---------------------------------------------------------------------------

/// Stub DashScope: submit returns a task id, the first poll is RUNNING, the second is
/// SUCCEEDED with an image URL, and the image URL serves PNG bytes.
async fn spawn_dashscope_stub(submit_delay: Duration) -> (SocketAddr, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let polls = Arc::new(AtomicUsize::new(0));
    let async_headers = Arc::new(AtomicUsize::new(0));
    let (polls_c, async_c) = (polls.clone(), async_headers.clone());

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let polls = polls_c.clone();
            let async_headers = async_c.clone();
            tokio::spawn(async move {
                let req = read_request(&mut stream).await;
                let first_line = req.lines().next().unwrap_or_default().to_string();

                if first_line.contains("/multimodal-generation/generation") {
                    if req.to_lowercase().contains("x-dashscope-async: enable") {
                        async_headers.fetch_add(1, Ordering::SeqCst);
                    }
                    tokio::time::sleep(submit_delay).await;
                    write_json(&mut stream, r#"{"output":{"task_id":"task-abc","task_status":"PENDING"}}"#).await;
                } else if first_line.contains("/api/v1/tasks/") {
                    let n = polls.fetch_add(1, Ordering::SeqCst) + 1;
                    if n < 2 {
                        write_json(&mut stream, r#"{"output":{"task_id":"task-abc","task_status":"RUNNING"}}"#).await;
                    } else {
                        let url = format!("http://{addr}/img/card.png");
                        let body = format!(
                            r#"{{"output":{{"task_id":"task-abc","task_status":"SUCCEEDED","results":[{{"url":"{url}"}}]}}}}"#
                        );
                        write_json(&mut stream, &body).await;
                    }
                } else {
                    // The image download.
                    let png: &[u8] = &[
                        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xDE, 0xAD, 0xBE, 0xEF,
                    ];
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(png).await;
                    let _ = stream.flush().await;
                }
            });
        }
    });

    (addr, polls, async_headers)
}

fn options_for(addr: SocketAddr) -> GenerationOptions {
    let mut provider_options = HashMap::new();
    provider_options.insert(
        "base_url".to_string(),
        serde_yaml::Value::String(format!("http://{addr}")),
    );
    GenerationOptions {
        model: "qwen-image-3.0".to_string(),
        aspect_ratio: Some("1:1".to_string()),
        negative_prompt: None,
        provider_options,
        verbose: false,
        duration_seconds: None,
        audio_kind: AudioKind::default(),
    }
}

#[tokio::test]
async fn async_task_mode_submits_polls_and_downloads() {
    std::env::set_var("MEDIA_QWEN_POLL_SECS", "1");
    std::env::set_var("MEDIA_QWEN_POLL_ATTEMPTS", "10");

    // A submit that itself takes a few seconds, to show the short submit leg is enough.
    let (addr, polls, async_headers) = spawn_dashscope_stub(Duration::from_secs(1)).await;
    let dir = std::env::temp_dir().join("media-tool-qwen-async");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("card.png");
    let _ = std::fs::remove_file(&out);

    let ok = QwenImageProvider
        .generate("a small test card", &out, "test-key", &options_for(addr), &[])
        .await
        .expect("provider must not error");

    assert!(ok, "async task mode should report success");
    assert_eq!(
        async_headers.load(Ordering::SeqCst),
        1,
        "submit must carry X-DashScope-Async: enable"
    );
    assert!(
        polls.load(Ordering::SeqCst) >= 2,
        "provider must keep polling past a RUNNING status"
    );
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..4], b"\x89PNG", "downloaded file should be the PNG");

    std::fs::remove_dir_all(&dir).ok();
}

/// A deployment that ignores the async header and answers inline must still work: the
/// provider takes the image instead of looking for a task id that will never arrive.
#[tokio::test]
async fn inline_response_is_used_without_polling() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let req = read_request(&mut stream).await;
                if req.contains("/multimodal-generation/generation") {
                    let url = format!("http://{addr}/img/inline.png");
                    let body = format!(
                        r#"{{"output":{{"choices":[{{"message":{{"content":[{{"image":"{url}"}}]}}}}]}}}}"#
                    );
                    write_json(&mut stream, &body).await;
                } else {
                    let png: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(png).await;
                    let _ = stream.flush().await;
                }
            });
        }
    });

    let dir = std::env::temp_dir().join("media-tool-qwen-inline");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("inline.png");
    let _ = std::fs::remove_file(&out);

    let ok = QwenImageProvider
        .generate("inline", &out, "test-key", &options_for(addr), &[])
        .await
        .expect("provider must not error");
    assert!(ok);
    assert_eq!(&std::fs::read(&out).unwrap()[..4], b"\x89PNG");

    std::fs::remove_dir_all(&dir).ok();
}
