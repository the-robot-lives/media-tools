//! Profile routing regression tests use local HTTP fixtures; no paid endpoints.
use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
use media_tool::llm_profiles::{self, ProfileStore};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

type Captured = Arc<Mutex<Vec<(HeaderMap, Value)>>>;
async fn handler(
    State((captured, response)): State<(Captured, Value)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    captured.lock().unwrap().push((headers, body));
    Json(response)
}
async fn server(path: &str, response: Value) -> (String, Captured, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let capture = Arc::new(Mutex::new(Vec::new()));
    let router = Router::new()
        .route(path, post(handler))
        .with_state((capture.clone(), response));
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{address}"), capture, task)
}
fn temp() -> PathBuf {
    let path = std::env::temp_dir().join(format!("media-profile-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[tokio::test]
async fn wire_protocols_and_keyless_local_connections() {
    for (provider, path, response) in [
        (
            "lmstudio",
            "/v1/chat/completions",
            json!({"choices":[{"message":{"content":"<ok/>"},"finish_reason":"stop"}]}),
        ),
        (
            "anthropic",
            "/v1/messages",
            json!({"content":[{"type":"text","text":"<ok/>"}],"stop_reason":"end_turn"}),
        ),
        (
            "ollama",
            "/api/chat",
            json!({"message":{"content":"<ok/>"},"done":true,"done_reason":"stop"}),
        ),
    ] {
        let (root, captured, task) = server(path, response).await;
        let mut p = llm_profiles::preset(provider);
        p.model = "qwen/exact-model".into();
        p.local = true;
        p.api_key_env = None;
        p.base_url = if provider == "ollama" {
            root
        } else {
            format!("{root}/v1")
        };
        let reply = llm_profiles::complete(&p, "system", "user", &p.model, &HashMap::new(), &[])
            .await
            .unwrap();
        assert_eq!(reply, "<ok/>");
        let calls = captured.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let (headers, body) = &calls[0];
        assert!(!headers.contains_key("authorization"));
        assert_eq!(body["model"], p.model);
        if provider == "anthropic" {
            assert_eq!(body["system"], "system");
            assert_eq!(body["messages"][0]["content"][0]["text"], "user");
        }
        if provider == "ollama" {
            assert_eq!(body["stream"], false);
            assert_eq!(body["options"]["num_predict"], 8192);
        }
        task.abort();
    }
}

#[tokio::test]
async fn truncated_response_fails_without_writing_a_document() {
    let (root, _, task) = server(
        "/v1/chat/completions",
        json!({"choices":[{"message":{"content":"<incomplete>"},"finish_reason":"length"}]}),
    )
    .await;
    let mut p = llm_profiles::preset("lmstudio");
    p.base_url = format!("{root}/v1");
    p.model = "qwen".into();
    assert!(
        llm_profiles::complete(&p, "s", "u", "qwen", &HashMap::new(), &[])
            .await
            .unwrap_err()
            .to_string()
            .contains("truncated")
    );
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompt_file_profile_overrides_pinned_cloud_provider_and_model() {
    let (root,captured,task)=server("/v1/chat/completions",json!({"choices":[{"message":{"content":"<catalog><book>local</book></catalog>"},"finish_reason":"stop"}]})).await;
    let dir = temp();
    let config = dir.join("profiles.yaml");
    let mut store = ProfileStore::default();
    let mut p = llm_profiles::preset("lmstudio");
    p.base_url = format!("{root}/v1");
    p.model = "qwen/exact-model".into();
    store.upsert("test-local".into(), p).unwrap();
    store.save_path(&config).unwrap();
    let prompt = dir.join("catalog.media.prompt");
    std::fs::write(&prompt,"schema: '0.4'\nid: local-xml\ntype: document\nservice: groq-chat\nmodel: never-use-cloud\nprompt:\n  text: Create an XML catalog.\noutput:\n  formats:\n    - format: xml\n  text_format: xml\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_generate-media-prompt"))
        .env("MEDIA_TOOL_LLM_CONFIG", &config)
        .env("MEDIA_TOOL_LLM_PROFILE", "test-local")
        .arg("--no-fim")
        .arg(&prompt)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("catalog.xml")).unwrap(),
        "<catalog><book>local</book></catalog>"
    );
    let calls = captured.lock().unwrap();
    assert_eq!(calls.len(), 1, "No cloud prep/eval or fallback requests");
    assert_eq!(calls[0].1["model"], "qwen/exact-model");
    task.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unknown_profile_and_noninteractive_tui_fail_actionably() {
    let dir = temp();
    let config = dir.join("profiles.yaml");
    let output = Command::new(env!("CARGO_BIN_EXE_generate-media-prompt"))
        .env("MEDIA_TOOL_LLM_CONFIG", &config)
        .env("MEDIA_TOOL_LLM_PROFILE", "missing")
        .args(["llm", "test"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unknown LLM profile"));
    let output = Command::new(env!("CARGO_BIN_EXE_generate-media-prompt"))
        .env("MEDIA_TOOL_LLM_CONFIG", &config)
        .args(["llm", "configure"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("needs a terminal"));
    assert!(!config.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authenticated_local_cli_profile_keeps_explicit_key_reference() {
    let dir = temp();
    let config = dir.join("profiles.yaml");
    let output = Command::new(env!("CARGO_BIN_EXE_generate-media-prompt"))
        .env("MEDIA_TOOL_LLM_CONFIG", &config)
        .args([
            "llm",
            "add",
            "authenticated-local",
            "--provider",
            "openai",
            "--local",
            "--api-key-env",
            "OPENAI_API_KEY",
            "--model",
            "qwen",
            "--base-url",
            "http://localhost:1234/v1",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let store = ProfileStore::load_path(&config).unwrap();
    assert_eq!(
        store.profiles["authenticated-local"].api_key_env.as_deref(),
        Some("OPENAI_API_KEY")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn modern_checkout_fim_guidance_is_injected_for_extended_asset_types() {
    let (root,captured,task)=server("/v1/chat/completions",json!({"choices":[{"message":{"content":"<!doctype html><html><body>fixture</body></html>"},"finish_reason":"stop"}]})).await;
    let dir = temp();
    let config = dir.join("profiles.yaml");
    let mut store = ProfileStore::default();
    let mut p = llm_profiles::preset("lmstudio");
    p.base_url = format!("{root}/v1");
    p.model = "qwen/exact-model".into();
    store.upsert("test-local".into(), p).unwrap();
    store.save_path(&config).unwrap();
    let prompt = dir.join("scene.media.prompt");
    std::fs::write(&prompt,"schema: '0.4'\nid: local-babylon\ntype: babylon.js\nprompt:\n  text: Create a cube scene.\noutput:\n  formats:\n    - format: html\n  text_format: babylon_js\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_generate-media-prompt"))
        .current_dir(&dir)
        .env("MEDIA_TOOL_LLM_CONFIG", &config)
        .env("MEDIA_TOOL_LLM_PROFILE", "test-local")
        .env_remove("MEDIA_FIM_DIR")
        .env_remove("MEDIA_FIM_INJECT")
        .arg(&prompt)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let calls = captured.lock().unwrap();
    let system = calls[0].1["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("FIM solution reference:"));
    assert!(system.contains("Babylon.js"));
    assert!(std::fs::read_to_string(dir.join("scene.html"))
        .unwrap()
        .contains("fixture"));
    task.abort();
    std::fs::remove_dir_all(dir).unwrap();
}
