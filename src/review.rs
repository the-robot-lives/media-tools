//! Read-only, exact-document review server. Native source stays separate from its viewer.
use axum::{
    extract::State,
    http::{header, HeaderValue},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use color_eyre::eyre::{bail, Context, Result};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const ADAPTERS: &str = include_str!("test_lab/static/review-adapters.js");
const PAGE: &str = include_str!("test_lab/static/review.html");
#[derive(Clone)]
struct Document {
    bytes: Arc<Vec<u8>>,
    filename: String,
    kind: String,
    mime: &'static str,
}

fn load(path: &Path, requested: Option<&str>) -> Result<Document> {
    let path = path
        .canonicalize()
        .wrap_err("Review document does not exist")?;
    if !path.is_file() {
        bail!("Review requires a file, not a directory");
    }
    let bytes = std::fs::read(&path).wrap_err("Cannot read review document")?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let kind = match requested {
        Some("music21j") => "music21j",
        Some("musicxml") | Some("osmd") => "musicxml",
        Some("text") => "text",
        Some("html") => "html",
        _ if ext == "html" || ext == "htm" => "html",
        _ if ext == "musicxml"
            || (ext == "xml"
                && (bytes.windows(15).any(|w| w == b"<score-partwise ")
                    || String::from_utf8_lossy(&bytes).contains("<score-partwise")
                    || String::from_utf8_lossy(&bytes).contains("<score-timewise"))) =>
        {
            "musicxml"
        }
        _ if filename.contains(".music21j.") => "music21j",
        _ if matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg"
        ) =>
        {
            "image"
        }
        _ if matches!(ext.as_str(), "mp3" | "wav" | "ogg" | "flac") => "audio",
        _ if matches!(ext.as_str(), "mp4" | "webm" | "mov") => "video",
        _ if ext == "pdf" => "pdf",
        _ => "text",
    }
    .to_string();
    let mime = match kind.as_str() {
        "music21j" => "text/javascript; charset=utf-8",
        "musicxml" => "application/xml; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "image" => match ext.as_str() {
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/jpeg",
        },
        "audio" => match ext.as_str() {
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "flac" => "audio/flac",
            _ => "audio/mpeg",
        },
        "video" => {
            if ext == "webm" {
                "video/webm"
            } else {
                "video/mp4"
            }
        }
        "pdf" => "application/pdf",
        _ => "text/plain; charset=utf-8",
    };
    Ok(Document {
        bytes: Arc::new(bytes),
        filename,
        kind,
        mime,
    })
}

async fn document(State(doc): State<Document>) -> Response {
    let mut response = doc.bytes.as_ref().clone().into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(doc.mime));
    // Sandboxed modules have an opaque origin; this route serves only the chosen document.
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn router(doc: Document) -> Router {
    Router::new()
        .route("/", get(|| async { Html(PAGE) }))
        .route(
            "/metadata",
            get(|State(doc): State<Document>| async move {
                Json(json!({"filename":doc.filename,"kind":doc.kind}))
            }),
        )
        .route("/document", get(document))
        .route(
            "/review-adapters.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    ADAPTERS,
                )
            }),
        )
        .with_state(doc)
}

pub async fn run(path: PathBuf, kind: Option<String>, port: u16, open: bool) -> Result<()> {
    let doc = load(&path, kind.as_deref())?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let url = format!("http://{}/", listener.local_addr()?);
    crate::telemetry::ok(&format!("Review {}: {url}", doc.filename));
    crate::telemetry::info("Ctrl+C closes the document viewer.");
    if open {
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(&url).spawn();
        #[cfg(target_os = "linux")]
        let result = std::process::Command::new("xdg-open").arg(&url).spawn();
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn();
        if result.is_err() {
            crate::telemetry::warn_msg(&format!("Open {url} in a browser"));
        }
    }
    axum::serve(listener, router(doc))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn serves_only_selected_document_and_no_arbitrary_file_routes() {
        let dir = std::env::temp_dir().join(format!("media-review-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("score.music21j.mjs");
        std::fs::write(
            &path,
            "export function buildScore(music21) {return new music21.stream.Stream();}",
        )
        .unwrap();
        let doc = load(&path, None).unwrap();
        assert_eq!(doc.kind, "music21j");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router(doc)).await.unwrap();
        });
        let client = reqwest::Client::new();
        let root = format!("http://{addr}");
        let response = client.get(format!("{root}/document")).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/javascript; charset=utf-8"
        );
        assert!(response.text().await.unwrap().contains("buildScore"));
        for route in ["/etc/passwd", "/api/media?path=/etc/passwd", "/other.mjs"] {
            assert_eq!(
                client
                    .get(format!("{root}{route}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                404
            );
        }
        assert_eq!(
            client
                .post(format!("{root}/document"))
                .send()
                .await
                .unwrap()
                .status(),
            405
        );
        let meta: serde_json::Value = client
            .get(format!("{root}/metadata"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(meta["filename"], "score.music21j.mjs");
        task.abort();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn file_checks_and_native_musicxml_detection() {
        assert!(load(Path::new("/this/document/does-not-exist"), None).is_err());
        assert!(load(&std::env::temp_dir(), None).is_err());
        let path = std::env::temp_dir().join(format!("score-{}.xml", uuid::Uuid::new_v4()));
        std::fs::write(
            &path,
            "<?xml version='1.0'?><score-partwise version='4.0'></score-partwise>",
        )
        .unwrap();
        assert_eq!(load(&path, None).unwrap().kind, "musicxml");
        std::fs::remove_file(path).unwrap();
    }
}
