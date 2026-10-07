//! Live DashScope tests for the qwen-image route split. **Ignored by default** — they call
//! the real endpoint, need a real key, and one of them deliberately waits out a ~60s
//! server-side cut.
//!
//! These exist because the stub-server suite could not have caught the real defect: a local
//! stub answers whenever we tell it to.
//!
//! Measured against the live endpoint, one heavy prompt on `multimodal-generation`:
//!
//! | Client | Protocol | Outcome |
//! |--------|----------|---------|
//! | curl x3 | HTTP/2 | `Error in the HTTP2 framing layer` at 61.7 / 61.5 / 61.5s |
//! | curl | HTTP/1.1 | HTTP 200 at 55.8s, then `Empty reply from server` at 61.1s |
//! | this crate (h1-only) | HTTP/1.1 | cut at 60.9s, then succeeded at 120.9s with one retry |
//! | this crate x3 | HTTP/1.1 | HTTP 200 at 46.8 / 53.7 / 57.0s |
//!
//! **The cut is not HTTP/2-specific.** That was the working theory for a while, because the
//! h2 framing error is loud and reproducible; but pinning curl to `--http1.1` only changes how
//! the same close is reported ("Empty reply from server"), and this crate cannot speak h2 at
//! all (the `h2` crate is not in the dependency graph) yet is cut at the same 61s mark.
//!
//! What the numbers actually say: DashScope closes a synchronous `multimodal-generation`
//! connection at ~61s, and one render's latency straddles that line, swinging between roughly
//! 45s and 75s with service load. The same request therefore succeeds or dies on a coin toss.
//! There is no client setting that moves a server-side close, so the mitigations are:
//!
//! * prompts without input images use the async-native `text2image/image-synthesis` route,
//!   which has no such limit and finished the same prompt in 9.6s and 14.9s;
//! * prompts *with* input images have no async route available, so a dropped connection is
//!   retried (default 3 attempts, `MEDIA_QWEN_RETRIES`).
//!
//! Run them with a key in the environment:
//!
//! ```sh
//! cargo test --test live_qwen_routes -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

use media_tool::providers::qwen_image::{route_for, text2image_model, QwenImageProvider, Route};
use media_tool::providers::{dashscope, http, GenerationOptions, MediaProvider};
use media_tool::schema::AudioKind;

/// A prompt heavy enough that the synchronous route cannot finish it inside the server's
/// ~60s ceiling. This is the shape that was failing in production.
const HEAVY_PROMPT: &str = "A technical orthographic presentation drawing of a human-identical \
android body, front elevation at left and side elevation at right, both full height, fine amber \
line work on a near-black ground with no shading fill. A partial cutaway on the front elevation \
reveals a carbon-composite skeleton beneath a silicone-elastomer skin in teal. Callout leader \
lines with empty callout boxes point to the wrist seam, the seam behind the ear, the shoulder \
joint and the hand. An inset detail circle at the lower right enlarges the left hand alone with \
a soft rose thermal wash. A faint violet measurement grid lies behind everything. All callout \
boxes, title blocks and dimension labels are empty: ruled boxes and leader lines only, no text \
or numerals anywhere. Precise technical illustration, patent-plate drafting, thin consistent \
line weights, orthographic elevations with cutaway and inset detail circle, no airbrush.";

fn key() -> String {
    dashscope::resolve_key()
        .expect("a DashScope key must be in the environment: DASHSCOPE_API_KEY / QWEN_API_KEY / QWEN_TOKEN_KEY")
}

fn heavy_options(route: &str) -> GenerationOptions {
    let mut provider_options = HashMap::new();
    provider_options.insert(
        "size".to_string(),
        serde_yaml::Value::String("1664x928".to_string()),
    );
    provider_options.insert(
        "route".to_string(),
        serde_yaml::Value::String(route.to_string()),
    );
    GenerationOptions {
        model: "qwen-image-3.0".to_string(),
        aspect_ratio: Some("16:9".to_string()),
        negative_prompt: Some("text, letters, words, numerals, watermark, logo".to_string()),
        provider_options,
        verbose: false,
        duration_seconds: None,
        audio_kind: AudioKind::default(),
    }
}

/// The measurement that justifies the whole change: a synchronous `multimodal-generation`
/// call carrying a heavy prompt sits right on the server's ~60s ceiling. It is a coin toss,
/// not a timeout we can tune \u2014 measured live, curl over HTTP/2 failed three for three at
/// 61.5-61.7s, this crate failed at ~62s, and a later attempt squeaked through at 57.6s.
///
/// This test deliberately does **not** assert that the sync route fails; asserting a bug
/// persists is a test that breaks when things improve. It asserts the shape of the failure
/// when one happens: never an instant error (that would be auth or a bad body), always up
/// against the ceiling. Each run prints its timings.
#[tokio::test]
#[ignore = "live: calls DashScope and may wait out a ~60s server-side cut, 3 attempts"]
async fn live_sync_multimodal_route_sits_on_the_sixty_second_ceiling() {
    let options = heavy_options("multimodal");
    assert_eq!(route_for(&options, false), Route::Multimodal);

    let url = dashscope::multimodal_url(&options);
    let body = serde_json::json!({
        "model": "qwen-image-3.0",
        "input": { "messages": [{ "role": "user", "content": [{ "text": HEAVY_PROMPT }] }] },
        "parameters": { "size": "1664*928" },
    });
    let key = key();

    let mut failures = 0;
    for attempt in 1..=3 {
        let client = http::client_with_timeout(Duration::from_secs(300));
        let started = Instant::now();
        let result = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", key))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(Duration::from_secs(300))
            .send()
            .await;
        let elapsed = started.elapsed();

        match result {
            Ok(r) => println!("sync attempt {attempt}: HTTP {} after {elapsed:?}", r.status()),
            Err(e) => {
                failures += 1;
                println!("sync attempt {attempt}: ERROR after {elapsed:?} \u{2014} {e}");
                assert!(
                    elapsed >= Duration::from_secs(45),
                    "attempt {attempt} failed after only {elapsed:?}: that is auth or request \
                     shape, not the server's ceiling"
                );
                assert!(
                    elapsed <= Duration::from_secs(120),
                    "attempt {attempt} failed after {elapsed:?}, far past the known ceiling"
                );
            }
        }
    }
    println!("sync multimodal: {failures}/3 attempts cut by the server");
}

/// The same heavy prompt on the async-native route completes, and well inside the window the
/// synchronous route could never have reached.
#[tokio::test]
#[ignore = "live: calls DashScope and spends one image credit"]
async fn live_async_text2image_route_renders_the_same_heavy_prompt() {
    let options = heavy_options("text2image");
    assert_eq!(route_for(&options, false), Route::Text2Image);
    assert_eq!(text2image_model("qwen-image-3.0"), "qwen-image");

    let dir = std::env::temp_dir().join("media-tool-live-qwen");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("heavy.png");
    let _ = std::fs::remove_file(&out);

    let started = Instant::now();
    let ok = QwenImageProvider
        .generate(HEAVY_PROMPT, &out, &key(), &options, &[])
        .await
        .expect("the async route must not error");
    let elapsed = started.elapsed();
    println!("async text2image: ok={ok} after {elapsed:?}");

    assert!(ok, "the async route should have produced an image");
    let bytes = std::fs::read(&out).expect("an image file should exist");
    assert_eq!(&bytes[..4], b"\x89PNG", "the download should be a PNG");
    assert!(bytes.len() > 10_000, "suspiciously small image: {} bytes", bytes.len());

    std::fs::remove_dir_all(&dir).ok();
}

/// Route selection, without touching the network.
#[test]
fn route_defaults_follow_attachments_and_overrides() {
    let mut options = heavy_options("text2image");
    assert_eq!(route_for(&options, false), Route::Text2Image);
    assert_eq!(route_for(&options, true), Route::Text2Image, "an explicit route wins");

    options.provider_options.remove("route");
    assert_eq!(route_for(&options, false), Route::Text2Image, "no images: async route");
    assert_eq!(
        route_for(&options, true),
        Route::Multimodal,
        "input images force the multimodal route, which is the only one that accepts them"
    );

    options.provider_options.insert(
        "route".to_string(),
        serde_yaml::Value::String("multimodal".to_string()),
    );
    assert_eq!(route_for(&options, false), Route::Multimodal);
}

/// `qwen-image-3.0` is a multimodal-only model id; text2image answers 400 InvalidParameter
/// for it, so the route maps it onto the text2image family.
#[test]
fn text2image_model_maps_the_multimodal_default() {
    assert_eq!(text2image_model("qwen-image-3.0"), "qwen-image");
    assert_eq!(text2image_model("default"), "qwen-image");
    assert_eq!(text2image_model(""), "qwen-image");
    assert_eq!(text2image_model("qwen-image-plus"), "qwen-image-plus");
    assert_eq!(text2image_model("wan2.2-t2i-flash"), "wan2.2-t2i-flash");
}

/// The multimodal route driven with a reference image, which is the path the async route
/// cannot serve: `text2image` accepts no input images, so image-to-image work has to use
/// `multimodal-generation` \u2014 exactly where the ~61s cut lives.
///
/// Measured: a single attempt was cut at 60.9s; with retry enabled the same call completed at
/// 120.9s, i.e. the first attempt died on the ceiling and the second got through. This test
/// exercises that retry path end to end.
#[tokio::test]
#[ignore = "live: calls DashScope with a reference image, may spend more than one credit"]
async fn live_multimodal_route_with_a_reference_image_retries_past_the_cut() {
    use media_tool::attachments::LoadedAttachment;
    use base64::Engine;

    let options = heavy_options("multimodal");
    assert_eq!(route_for(&options, true), Route::Multimodal);

    // A small synthetic reference image, so the test carries no fixture.
    let mut png = Vec::new();
    {
        let mut buf = image::RgbaImage::new(512, 512);
        for (x, y, px) in buf.enumerate_pixels_mut() {
            let v = ((x / 32 + y / 32) % 2) as u8;
            *px = image::Rgba([v * 200, 120, 255 - v * 100, 255]);
        }
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
    }
    let attachment = LoadedAttachment {
        path: "synthetic.png".to_string(),
        role: "reference".to_string(),
        mime_type: "image/png".to_string(),
        description: "checkerboard reference".to_string(),
        data_b64: base64::engine::general_purpose::STANDARD.encode(&png),
    };

    let dir = std::env::temp_dir().join("media-tool-live-qwen-img2img");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("img2img.png");
    let _ = std::fs::remove_file(&out);

    let started = Instant::now();
    let result = QwenImageProvider
        .generate(HEAVY_PROMPT, &out, &key(), &options, &[attachment])
        .await;
    let elapsed = started.elapsed();
    println!("multimodal + reference image (http1): {result:?} after {elapsed:?}");

    let ok = result.expect("the multimodal route must not error");
    assert!(ok, "the render should have produced an image");
    let bytes = std::fs::read(&out).expect("an image file should exist");
    assert_eq!(&bytes[..4], b"\x89PNG");
    assert!(bytes.len() > 10_000, "suspiciously small image: {} bytes", bytes.len());

    std::fs::remove_dir_all(&dir).ok();
}
