//! End-to-end check of the `post_processing:` YAML shape used by real prompt files:
//! the exact crop+resize pair an Open Graph card uses must turn a 1024x1024 provider
//! output into a 1200x630 file on disk.

use image::{DynamicImage, Rgba, RgbaImage};
use media_tool::postprocess;
use media_tool::schema::PostProcessStep;

fn square(size: u32) -> DynamicImage {
    let mut buf = RgbaImage::new(size, size);
    for (x, y, px) in buf.enumerate_pixels_mut() {
        *px = Rgba([(x % 256) as u8, (y % 256) as u8, 200, 255]);
    }
    DynamicImage::ImageRgba8(buf)
}

#[test]
fn og_card_steps_produce_an_exact_1200x630_png() {
    let steps: Vec<PostProcessStep> = serde_yaml::from_str(
        r#"
- action: crop
  params:
    gravity: center
    aspect_ratio: "1.91:1"
- action: resize
  params:
    width: 1200
    height: 630
    fit: cover
"#,
    )
    .expect("post_processing block parses");
    assert_eq!(steps.len(), 2);

    let dir = std::env::temp_dir().join("media-tool-og-card-steps");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("og-card.png");
    square(1024).save(&path).unwrap();

    for step in &steps {
        assert!(postprocess::handles(&step.action), "{}", step.action);
        postprocess::apply_to_file(&path, &step.action, &step.params)
            .unwrap_or_else(|e| panic!("step {} failed: {}", step.action, e));
    }

    let out = image::open(&path).unwrap();
    assert_eq!((out.width(), out.height()), (1200, 630));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unimplemented_action_is_not_silently_accepted() {
    let step: PostProcessStep = serde_yaml::from_str("action: optimize\nparams: {}\n").unwrap();
    assert!(!postprocess::handles(&step.action));
}
