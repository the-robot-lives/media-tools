use std::process::Command;

#[test]
fn models_command_is_offline_and_lists_policy() {
    let result = Command::new(env!("CARGO_BIN_EXE_generate-media-prompt"))
        .arg("models")
        .env("MEDIA_TOOL_CONFIG", "http://127.0.0.1:1/never-fetch")
        .output()
        .unwrap();
    assert!(result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("gemini-3.1-flash-lite-image"));
    assert!(output.contains("gemini-3-pro-image           supported"));
    assert!(output.contains("gemini-2.5-flash-image       blocked"));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("fetch"));
}

#[test]
fn dry_run_rejects_old_models_and_provider_overrides_without_credentials() {
    let dir = std::env::temp_dir().join(format!("media-policy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (model, override_line, expected_success) in [
        ("gemini-2.5-flash-image", "", false),
        ("gemini-1.5-flash-image", "", false),
        ("nano-banana", "", false),
        (
            "gemini-3.1-flash-image",
            "  provider_options:\n    generate_content_model: gemini-2.5-flash-image\n",
            false,
        ),
        ("gemini-3.1-flash-image", "", true),
        // The highest-quality model in the lineup carries no minor version and must
        // pass; a `>= 3.1` reading of the policy would wrongly reject it.
        ("gemini-3-pro-image", "", true),
    ] {
        let prompt = dir.join("policy.media.prompt");
        std::fs::write(&prompt, format!("schema: '0.4'\nid: policy\ntype: image\nservice: gemini\nmodel: {model}\nprompt:\n  text: test image\n{override_line}output:\n  formats: [{{format: png}}]\n")).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_generate-media-prompt"))
            .args(["--dry-run", "--no-prep", "--no-eval"])
            .arg(&prompt)
            .env("MEDIA_TOOL_CONFIG", dir.join("missing.yaml"))
            .env_remove("MEDIA_TOOL_CONFIG_URL")
            .env_remove("GEMINI_API_KEY")
            .output()
            .unwrap();
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            result.status.success(),
            expected_success,
            "{model}: {output}"
        );
        if !expected_success {
            assert!(output.contains("Use Gemini 3 or newer"), "{output}");
        }
    }
    // CLI override and configured fallback must be rejected even with no key.
    let prompt = dir.join("policy.media.prompt");
    let config = dir.join("config.yaml");
    std::fs::write(
        &config,
        "image_tiers:\n  medium:\n    - gemini:gemini-2.5-flash-image\n",
    )
    .unwrap();
    for cli_override in [true, false] {
        std::fs::write(&prompt, "schema: '0.4'\nid: auto-policy\ntype: image\nprompt:\n  text: test\noutput:\n  formats: [{format: png}]\n").unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_generate-media-prompt"));
        command
            .arg("--dry-run")
            .arg(&prompt)
            .env("MEDIA_TOOL_CONFIG", &config);
        if cli_override {
            command.args(["--service", "gemini", "--model", "gemini-2.5-flash-image"]);
        }
        let result = command.output().unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("Use Gemini 3 or newer"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}
