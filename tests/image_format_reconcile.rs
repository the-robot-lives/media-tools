//! The gemini image provider returns JPEG bytes whatever `output.format` a prompt declares.
//!
//! Before this fix, `format: png` wrote JPEG bytes into a `.png` file, the run reported
//! success, and post-processing then failed with "cannot decode" — the only honest signal
//! arrived a step too late, after the run had already claimed to work.
//!
//! These tests drive the real provider against a stub that reproduces exactly that: a
//! `generateContent` response whose `inlineData` is JPEG, requested at a `.png` path.

use std::collections::HashMap;
use std::net::SocketAddr;

use base64::Engine;
use media_tool::imagefmt::{self, ImageKind};
use media_tool::providers::gemini::GeminiProvider;
use media_tool::providers::{GenerationOptions, MediaProvider};
use media_tool::schema::AudioKind;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
    let mut buf = image::RgbImage::new(w, h);
    for (x, y, px) in buf.enumerate_pixels_mut() {
        *px = image::Rgb([(x % 256) as u8, (y % 256) as u8, 140]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(buf)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg)
        .unwrap();
    out
}

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
            let content_len = text[..head_end]
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

/// A stub gemini that always answers with JPEG bytes, whatever was asked for.
async fn spawn_jpeg_gemini() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let _ = read_request(&mut stream).await;
                let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg_bytes(320, 240));
                let body = format!(
                    r#"{{"candidates":[{{"content":{{"parts":[{{"inlineData":{{"mimeType":"image/jpeg","data":"{b64}"}}}}]}}}}]}}"#
                );
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.flush().await;
            });
        }
    });
    addr
}

fn options_for(addr: SocketAddr) -> GenerationOptions {
    let mut provider_options = HashMap::new();
    provider_options.insert(
        "base_url".to_string(),
        serde_yaml::Value::String(format!("http://{addr}")),
    );
    // Force the generateContent path, which is where inlineData comes back.
    provider_options.insert(
        "generate_content_model".to_string(),
        serde_yaml::Value::String("gemini-3.1-flash-lite-image".to_string()),
    );
    GenerationOptions {
        model: "gemini-3.1-flash-lite-image".to_string(),
        aspect_ratio: Some("4:3".to_string()),
        negative_prompt: None,
        provider_options,
        verbose: false,
        duration_seconds: None,
        audio_kind: AudioKind::default(),
    }
}

/// End to end: the provider is asked for a `.png`, the stub returns JPEG, and what lands on
/// disk must be a real PNG that post-processing can decode.
#[tokio::test]
async fn jpeg_from_gemini_under_a_png_declaration_lands_as_a_real_png() {
    let addr = spawn_jpeg_gemini().await;
    let dir = std::env::temp_dir().join("media-tool-gemini-jpeg-png");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("card.png");
    let _ = std::fs::remove_file(&out);

    // A reference image forces the generateContent branch.
    let attachment = media_tool::attachments::LoadedAttachment {
        path: "ref.png".to_string(),
        role: "reference".to_string(),
        mime_type: "image/png".to_string(),
        description: "reference".to_string(),
        data_b64: base64::engine::general_purpose::STANDARD.encode(jpeg_bytes(16, 16)),
    };

    let ok = GeminiProvider
        .generate("a card", &out, "test-key", &options_for(addr), &[attachment])
        .await
        .expect("the provider must not error");
    assert!(ok, "generation should report success");

    let on_disk = std::fs::read(&out).expect("the declared .png path should exist");
    assert_eq!(
        imagefmt::sniff(&on_disk),
        Some(ImageKind::Png),
        "the file named .png must actually be a PNG"
    );

    // The step that used to fail: post-processing must be able to decode it.
    let mut params = HashMap::new();
    params.insert(
        "aspect_ratio".to_string(),
        serde_yaml::Value::String("1:1".to_string()),
    );
    let (w, h) = media_tool::postprocess::apply_to_file(&out, "crop", &params)
        .expect("post-processing must decode the written file");
    assert_eq!(w, h, "a 1:1 crop should be square, got {w}x{h}");

    std::fs::remove_dir_all(&dir).ok();
}

/// Post-processing decodes by content, so even a file whose extension lies is handled rather
/// than reported as corrupt.
#[tokio::test]
async fn post_processing_decodes_by_content_not_extension() {
    let dir = std::env::temp_dir().join("media-tool-mislabelled");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mislabelled.png");
    // Deliberately bypass the reconciling writer: raw JPEG bytes under a .png name.
    std::fs::write(&path, jpeg_bytes(200, 100)).unwrap();

    let mut params = HashMap::new();
    params.insert(
        "width".to_string(),
        serde_yaml::Value::String("50".to_string()),
    );
    params.insert(
        "height".to_string(),
        serde_yaml::Value::String("50".to_string()),
    );
    let (w, h) = media_tool::postprocess::apply_to_file(&path, "resize", &params)
        .expect("a mislabelled file should still decode");
    assert_eq!((w, h), (50, 50));

    std::fs::remove_dir_all(&dir).ok();
}
