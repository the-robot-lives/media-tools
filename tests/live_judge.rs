//! Live judge smoke tests against the real Alibaba and Jev backends.
//!
//! `#[ignore]`d by default (paid endpoints, needs resolved credentials). Run
//! explicitly when keys/base URLs resolve:
//!
//! ```sh
//! cargo test --test live_judge -- --ignored --nocapture
//! ```
//!
//! Credentials resolve through the normal chain (env first, dc store fallback);
//! when nothing resolves the test prints the reason and skips.

use media_tool::judge::{
    build_rubric, classify, output_kind, JudgeConfig, Question, QuestionKind, ScoreLevel,
};
use serde_json::json;

const FIXTURE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <rect x="2" y="2" width="20" height="20" rx="4" fill="#0A1628"/>
  <circle cx="12" cy="12" r="7" fill="none" stroke="#00D4FF" stroke-width="2"/>
  <path d="M9 12l2 2 4-4" fill="none" stroke="#00D4FF" stroke-width="2"/>
</svg>"##;

fn rubric_questions() -> Vec<Question> {
    let levels: Vec<ScoreLevel> = (0..10)
        .map(|index| ScoreLevel {
            score: index as f64 / 9.0,
            label: format!(
                "{index} — SVG quality level {index} of 9 (0 = broken markup, 9 = clean, valid, on-brief icon)"
            ),
        })
        .collect();
    vec![
        Question::new("quality", QuestionKind::Score(levels))
            .with_instructions("Rate the SVG icon against the rubric levels."),
        Question::new("uses_viewbox", QuestionKind::Noul)
            .with_instructions("Answer true if the SVG declares a viewBox attribute."),
    ]
}

fn resolved_config(backend: &str) -> Result<JudgeConfig, String> {
    JudgeConfig::resolve(Some(backend), None, None).map_err(|err| format!("{err:#}"))
}

#[tokio::test]
#[ignore]
async fn alibaba_decision_model_live() {
    let cfg = match resolved_config("alibaba") {
        Ok(cfg) => cfg,
        Err(reason) => {
            println!("SKIP alibaba: {reason}");
            return;
        }
    };
    println!(
        "alibaba backend: {} base={} model={}",
        cfg.backend.as_str(),
        cfg.base,
        cfg.model
    );
    assert_eq!(output_kind("svg"), media_tool::judge::OutputKind::Text);
    let questions = rubric_questions();
    let result = classify(&cfg, &json!({"output": FIXTURE_SVG}), &questions)
        .await
        .expect("alibaba classify");
    println!("request_id: {:?}", result.request_id);
    println!("model: {}", result.model);
    println!("usage: {:?}", result.usage);
    for answer in &result.answers {
        println!(
            "{}: {:?} probabilities={:?}",
            answer.question_id, answer.kind, answer.distribution
        );
    }
    assert_eq!(result.answers.len(), questions.len());
}

#[tokio::test]
#[ignore]
async fn jev_live() {
    let cfg = match resolved_config("jev") {
        Ok(cfg) => cfg,
        Err(reason) => {
            println!("SKIP jev: {reason}");
            return;
        }
    };
    println!("jev backend: {} base={} model={}", cfg.backend.as_str(), cfg.base, cfg.model);
    let questions = rubric_questions();
    let result = classify(&cfg, &json!({"output": FIXTURE_SVG}), &questions)
        .await
        .expect("jev classify");
    println!("request_id: {:?}", result.request_id);
    println!("model: {}", result.model);
    println!("usage: {:?}", result.usage);
    for answer in &result.answers {
        println!(
            "{}: {:?} probabilities={:?}",
            answer.question_id, answer.kind, answer.distribution
        );
    }
    assert_eq!(result.answers.len(), questions.len());
}

/// Sanity: the rubric built for the CLI smoke is well-formed wire input.
#[test]
fn live_rubric_encodes() {
    let eval = media_tool::schema::EvalSection {
        judge: None,
        pass_threshold: Some(0.7),
        max_attempts: None,
        required_pass: vec!["quality".into()],
        criteria: [(
            "quality".to_string(),
            media_tool::schema::EvalCriterion {
                weight: Some(1.0),
                scale: None,
                description: Some("clean, valid, on-brief SVG icon".into()),
                fail_signals: vec!["malformed markup".into()],
            },
        )]
        .into_iter()
        .collect(),
        reject_if: vec!["rasterized content".into()],
        mode: None,
        visual: None,
    };
    let rubric = build_rubric(&eval);
    let wire = media_tool::judge::system_one::encode(
        "decision-model-preview",
        &json!({"output": FIXTURE_SVG}),
        &rubric.questions,
    )
    .expect("encode");
    assert_eq!(wire.questions.len(), 2);
}
