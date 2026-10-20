//! Registry integration check for the chat-generated asset types added in the
//! new-asset-types wave: `cli-tool-mockup`, `js-shell`, `text-video`.
//!
//! Exercises the public library surface the way a .media.prompt run does:
//! type-string parsing, default extensions, chat routing, prep channels and
//! static provider guidance — no network, no API keys.

use media_tool::prep::{prep_channel, provider_prompt_guidance};
use media_tool::providers::candidates_for;
use media_tool::schema::{AssetType, AudioKind, Quality};

#[test]
fn new_types_parse_to_chat_types_with_expected_extensions() {
    let cases = [
        ("cli-tool-mockup", AssetType::CliToolMockup, "html"),
        ("js-shell", AssetType::JsShell, "mjs"),
        ("text-video", AssetType::TextVideo, "tape"),
    ];
    for (type_str, expected, ext) in cases {
        let (asset, audio) = AssetType::from_type_str(type_str);
        assert_eq!(asset, expected, "type string '{type_str}'");
        assert_eq!(audio, AudioKind::Voice);
        assert_eq!(asset.default_extension(), ext);
        assert!(asset.is_chat_type());
    }
}

#[test]
fn new_types_route_through_the_chat_ladder() {
    for asset in [
        AssetType::CliToolMockup,
        AssetType::JsShell,
        AssetType::TextVideo,
    ] {
        for q in [Quality::Low, Quality::Medium, Quality::High] {
            let cands = candidates_for(asset, AudioKind::Voice, q);
            assert!(
                cands.iter().all(|c| c.service == "groq-chat"),
                "{asset:?}/{q:?} must ride the chat ladder, got {:?}",
                cands.iter().map(|c| c.service).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn new_types_map_to_prep_channels() {
    assert_eq!(
        prep_channel(
            AssetType::CliToolMockup,
            AudioKind::Voice,
            None,
            "groq-chat"
        ),
        media_tool::prep::PrepChannel::Html
    );
    assert_eq!(
        prep_channel(AssetType::JsShell, AudioKind::Voice, None, "groq-chat"),
        media_tool::prep::PrepChannel::Code
    );
    assert_eq!(
        prep_channel(AssetType::TextVideo, AudioKind::Voice, None, "groq-chat"),
        media_tool::prep::PrepChannel::Diagram
    );
}

#[test]
fn new_types_have_static_provider_guidance() {
    // The mockup guidance must carry the canonical screens-doc grammar.
    let mockup = provider_prompt_guidance("groq-chat", AssetType::CliToolMockup, None);
    for needle in [
        "noizu-cli-mockup",
        "@screen",
        "[[<label> -> #<target-id>]]",
        "[[<label> -> run:<command>]]",
        "[[<label> -> #<target-id>|run:<command>]]",
        r"\[[",
    ] {
        assert!(mockup.contains(needle), "mockup guidance missing {needle}");
    }

    let js = provider_prompt_guidance("groq-chat", AssetType::JsShell, None);
    assert!(js.contains("CommandSpec") && js.contains("import time"));

    let tape = provider_prompt_guidance("groq-chat", AssetType::TextVideo, None);
    for needle in ["Output", "Settings", "Typeset", "Type", "Require", "vhs"] {
        assert!(tape.contains(needle), "tape guidance missing {needle}");
    }
}

#[test]
fn vhs_renderer_is_registered_and_reports_availability() {
    let renderer = media_tool::renderers::get_renderer("vhs");
    assert!(renderer.is_some(), "vhs renderer must be registered");
    // Available or not (vhs binary on PATH or not), asking must not panic —
    // the graceful-skip path keeps CI and offline machines safe.
    let _ = renderer.unwrap().is_available();
}
