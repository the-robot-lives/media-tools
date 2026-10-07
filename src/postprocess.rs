//! Image post-processing steps (`crop`, `resize`) for `post_processing:` entries in a
//! `.media.prompt` file.
//!
//! These operate on files already written by a provider: the image is decoded, transformed
//! and re-encoded in place, preserving the original file extension/format. Everything here
//! is pure and synchronous so it can be unit-tested without a provider call.
//!
//! Supported actions:
//!
//! * `crop` — `gravity` (center|top|bottom|left|right), `aspect_ratio` ("1.91:1", "16:9",
//!   "4:3", "1:1"), or an explicit `width`/`height` box.
//! * `resize` — `width`, `height`, `fit` (cover|contain|fill).
//!
//! Any other action is *not* silently skipped; the caller treats it as a failure.

use std::collections::HashMap;
use std::path::Path;

use color_eyre::eyre::{bail, eyre, WrapErr};
use image::imageops::FilterType;
use image::DynamicImage;

pub type Params = HashMap<String, serde_yaml::Value>;

/// Anchor used when a crop discards pixels along an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gravity {
    #[default]
    Center,
    Top,
    Bottom,
    Left,
    Right,
}

impl Gravity {
    pub fn parse(s: &str) -> color_eyre::Result<Gravity> {
        match s.trim().to_ascii_lowercase().as_str() {
            "center" | "centre" | "middle" => Ok(Gravity::Center),
            "top" | "north" => Ok(Gravity::Top),
            "bottom" | "south" => Ok(Gravity::Bottom),
            "left" | "west" => Ok(Gravity::Left),
            "right" | "east" => Ok(Gravity::Right),
            other => Err(eyre!(
                "unknown crop gravity '{}' (expected center|top|bottom|left|right)",
                other
            )),
        }
    }
}

/// How `resize` reconciles the source aspect with the requested box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Crop to the target aspect (gravity-anchored), then scale to exactly width×height.
    #[default]
    Cover,
    /// Scale down/up to fit *within* the box, preserving aspect. No letterbox padding.
    Contain,
    /// Stretch to exactly width×height, ignoring aspect.
    Fill,
}

impl Fit {
    pub fn parse(s: &str) -> color_eyre::Result<Fit> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cover" => Ok(Fit::Cover),
            "contain" | "inside" => Ok(Fit::Contain),
            "fill" | "stretch" | "exact" => Ok(Fit::Fill),
            other => Err(eyre!(
                "unknown resize fit '{}' (expected cover|contain|fill)",
                other
            )),
        }
    }
}

/// Parse `"1.91:1"`, `"16:9"`, `"16/9"` or a bare `"1.91"` into a width/height ratio.
pub fn parse_aspect_ratio(s: &str) -> color_eyre::Result<f64> {
    let raw = s.trim();
    let (w, h) = match raw.split_once(|c| c == ':' || c == '/' || c == 'x') {
        Some((w, h)) => (w.trim(), h.trim()),
        None => (raw, "1"),
    };
    let w: f64 = w
        .parse()
        .wrap_err_with(|| format!("bad aspect_ratio '{}'", raw))?;
    let h: f64 = h
        .parse()
        .wrap_err_with(|| format!("bad aspect_ratio '{}'", raw))?;
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        bail!("aspect_ratio '{}' must be positive and finite", raw);
    }
    Ok(w / h)
}

/// The largest `aspect`-ratio box that fits inside `src_w`×`src_h`, anchored by `gravity`.
///
/// Returns `(x, y, w, h)` in pixels.
pub fn crop_box(src_w: u32, src_h: u32, aspect: f64, gravity: Gravity) -> (u32, u32, u32, u32) {
    let (sw, sh) = (src_w as f64, src_h as f64);
    let src_aspect = sw / sh;

    let (mut w, mut h) = if src_aspect > aspect {
        // Source is too wide — keep full height, trim width.
        ((sh * aspect).round(), sh)
    } else {
        // Source is too tall — keep full width, trim height.
        (sw, (sw / aspect).round())
    };
    w = w.clamp(1.0, sw);
    h = h.clamp(1.0, sh);

    let (w, h) = (w as u32, h as u32);
    let (x, y) = offsets(src_w, src_h, w, h, gravity);
    (x, y, w, h)
}

/// Anchor a `w`×`h` window inside `src_w`×`src_h`.
fn offsets(src_w: u32, src_h: u32, w: u32, h: u32, gravity: Gravity) -> (u32, u32) {
    let slack_x = src_w.saturating_sub(w);
    let slack_y = src_h.saturating_sub(h);
    match gravity {
        Gravity::Center => (slack_x / 2, slack_y / 2),
        Gravity::Top => (slack_x / 2, 0),
        Gravity::Bottom => (slack_x / 2, slack_y),
        Gravity::Left => (0, slack_y / 2),
        Gravity::Right => (slack_x, slack_y / 2),
    }
}

fn param_str(params: &Params, key: &str) -> Option<String> {
    params.get(key).and_then(|v| match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn param_u32(params: &Params, key: &str) -> color_eyre::Result<Option<u32>> {
    match params.get(key) {
        None | Some(serde_yaml::Value::Null) => Ok(None),
        Some(serde_yaml::Value::Number(n)) => {
            let v = n
                .as_f64()
                .ok_or_else(|| eyre!("param '{}' is not a number", key))?;
            if v < 1.0 {
                bail!("param '{}' must be >= 1 (got {})", key, v);
            }
            Ok(Some(v.round() as u32))
        }
        Some(serde_yaml::Value::String(s)) => {
            let v: u32 = s
                .trim()
                .parse()
                .wrap_err_with(|| format!("param '{}' is not an integer: '{}'", key, s))?;
            if v < 1 {
                bail!("param '{}' must be >= 1", key);
            }
            Ok(Some(v))
        }
        Some(other) => bail!("param '{}' has unsupported type: {:?}", key, other),
    }
}

fn gravity_of(params: &Params) -> color_eyre::Result<Gravity> {
    match param_str(params, "gravity") {
        Some(s) => Gravity::parse(&s),
        None => Ok(Gravity::Center),
    }
}

/// Apply a `crop` step to a decoded image.
pub fn apply_crop(img: &DynamicImage, params: &Params) -> color_eyre::Result<DynamicImage> {
    let gravity = gravity_of(params)?;
    let (sw, sh) = (img.width(), img.height());

    let width = param_u32(params, "width")?;
    let height = param_u32(params, "height")?;

    let (x, y, w, h) = match (width, height) {
        // Explicit box wins over aspect_ratio.
        (Some(w), Some(h)) => {
            let w = w.min(sw);
            let h = h.min(sh);
            let (x, y) = offsets(sw, sh, w, h, gravity);
            (x, y, w, h)
        }
        _ => {
            let aspect = match param_str(params, "aspect_ratio") {
                Some(a) => parse_aspect_ratio(&a)?,
                None => match (width, height) {
                    (Some(w), None) => {
                        let w = w.min(sw);
                        let (x, y) = offsets(sw, sh, w, sh, gravity);
                        return Ok(img.crop_imm(x, y, w, sh));
                    }
                    (None, Some(h)) => {
                        let h = h.min(sh);
                        let (x, y) = offsets(sw, sh, sw, h, gravity);
                        return Ok(img.crop_imm(x, y, sw, h));
                    }
                    _ => bail!("crop requires 'aspect_ratio' or 'width'/'height'"),
                },
            };
            crop_box(sw, sh, aspect, gravity)
        }
    };

    Ok(img.crop_imm(x, y, w, h))
}

/// Apply a `resize` step to a decoded image.
pub fn apply_resize(img: &DynamicImage, params: &Params) -> color_eyre::Result<DynamicImage> {
    let fit = match param_str(params, "fit") {
        Some(s) => Fit::parse(&s)?,
        None => Fit::Cover,
    };
    let gravity = gravity_of(params)?;
    let width = param_u32(params, "width")?;
    let height = param_u32(params, "height")?;

    let (sw, sh) = (img.width(), img.height());

    match (width, height, fit) {
        (None, None, _) => bail!("resize requires 'width' and/or 'height'"),

        // Single-axis target: preserve aspect regardless of fit.
        (Some(w), None, _) => {
            let h = ((w as f64) * (sh as f64) / (sw as f64)).round().max(1.0) as u32;
            Ok(img.resize_exact(w, h, FilterType::Lanczos3))
        }
        (None, Some(h), _) => {
            let w = ((h as f64) * (sw as f64) / (sh as f64)).round().max(1.0) as u32;
            Ok(img.resize_exact(w, h, FilterType::Lanczos3))
        }

        (Some(w), Some(h), Fit::Fill) => Ok(img.resize_exact(w, h, FilterType::Lanczos3)),
        (Some(w), Some(h), Fit::Contain) => Ok(img.resize(w, h, FilterType::Lanczos3)),
        (Some(w), Some(h), Fit::Cover) => {
            let aspect = (w as f64) / (h as f64);
            let (x, y, cw, ch) = crop_box(sw, sh, aspect, gravity);
            let cropped = img.crop_imm(x, y, cw, ch);
            Ok(cropped.resize_exact(w, h, FilterType::Lanczos3))
        }
    }
}

/// Is `action` an image post-processing step this module handles?
pub fn handles(action: &str) -> bool {
    matches!(action, "crop" | "resize")
}

/// Run a `crop` or `resize` step against a file, rewriting it in place.
///
/// Returns the resulting `(width, height)`.
pub fn apply_to_file(
    path: &Path,
    action: &str,
    params: &Params,
) -> color_eyre::Result<(u32, u32)> {
    // Decode by *content*, not by extension. A provider may hand back a container other than
    // the one the prompt declared, and `image::open` guesses from the file name.
    let img = image::ImageReader::open(path)
        .wrap_err_with(|| format!("post-processing '{}': cannot open {}", action, path.display()))?
        .with_guessed_format()
        .wrap_err_with(|| {
            format!("post-processing '{}': cannot read {}", action, path.display())
        })?
        .decode()
        .wrap_err_with(|| {
            format!("post-processing '{}': cannot decode {}", action, path.display())
        })?;

    let out = match action {
        "crop" => apply_crop(&img, params)?,
        "resize" => apply_resize(&img, params)?,
        other => bail!("post-processing action '{}' is not an image step", other),
    };

    out.save(path)
        .wrap_err_with(|| format!("post-processing '{}': cannot write {}", action, path.display()))?;

    Ok((out.width(), out.height()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn img(w: u32, h: u32) -> DynamicImage {
        let mut buf = RgbaImage::new(w, h);
        for (x, y, px) in buf.enumerate_pixels_mut() {
            *px = Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]);
        }
        DynamicImage::ImageRgba8(buf)
    }

    fn params(pairs: &[(&str, &str)]) -> Params {
        pairs
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    serde_yaml::Value::String(v.to_string()),
                )
            })
            .collect()
    }

    #[test]
    fn parses_aspect_ratios() {
        assert!((parse_aspect_ratio("1.91:1").unwrap() - 1.91).abs() < 1e-9);
        assert!((parse_aspect_ratio("16:9").unwrap() - 16.0 / 9.0).abs() < 1e-9);
        assert!((parse_aspect_ratio("4:3").unwrap() - 4.0 / 3.0).abs() < 1e-9);
        assert_eq!(parse_aspect_ratio("1:1").unwrap(), 1.0);
        assert_eq!(parse_aspect_ratio("2").unwrap(), 2.0);
        assert!(parse_aspect_ratio("0:1").is_err());
        assert!(parse_aspect_ratio("wide").is_err());
    }

    #[test]
    fn crop_box_is_centered_and_aspect_correct() {
        // 1024x1024 → 1.91:1 keeps full width, trims height.
        let (x, y, w, h) = crop_box(1024, 1024, 1.91, Gravity::Center);
        assert_eq!((x, w), (0, 1024));
        assert_eq!(h, 536); // 1024 / 1.91 = 536.1
        assert_eq!(y, (1024 - 536) / 2);
    }

    #[test]
    fn crop_box_honors_gravity() {
        assert_eq!(crop_box(1000, 1000, 2.0, Gravity::Top).1, 0);
        assert_eq!(crop_box(1000, 1000, 2.0, Gravity::Bottom).1, 500);
        assert_eq!(crop_box(1000, 1000, 0.5, Gravity::Left).0, 0);
        assert_eq!(crop_box(1000, 1000, 0.5, Gravity::Right).0, 500);
    }

    #[test]
    fn crop_to_og_aspect() {
        let out = apply_crop(&img(1024, 1024), &params(&[("aspect_ratio", "1.91:1"), ("gravity", "center")])).unwrap();
        assert_eq!(out.width(), 1024);
        assert_eq!(out.height(), 536);
        let ratio = out.width() as f64 / out.height() as f64;
        assert!((ratio - 1.91).abs() < 0.01, "ratio was {}", ratio);
    }

    #[test]
    fn crop_explicit_box() {
        let out = apply_crop(&img(800, 600), &params(&[("width", "400"), ("height", "300")])).unwrap();
        assert_eq!((out.width(), out.height()), (400, 300));
    }

    #[test]
    fn crop_rejects_missing_params() {
        assert!(apply_crop(&img(64, 64), &params(&[])).is_err());
    }

    #[test]
    fn resize_cover_hits_exact_box() {
        let out = apply_resize(&img(1024, 1024), &params(&[("width", "1200"), ("height", "630"), ("fit", "cover")])).unwrap();
        assert_eq!((out.width(), out.height()), (1200, 630));
    }

    #[test]
    fn resize_contain_preserves_aspect_inside_box() {
        let out = apply_resize(&img(1024, 512), &params(&[("width", "600"), ("height", "600"), ("fit", "contain")])).unwrap();
        assert_eq!((out.width(), out.height()), (600, 300));
    }

    #[test]
    fn resize_fill_stretches() {
        let out = apply_resize(&img(1024, 1024), &params(&[("width", "1200"), ("height", "630"), ("fit", "fill")])).unwrap();
        assert_eq!((out.width(), out.height()), (1200, 630));
    }

    #[test]
    fn resize_single_axis_preserves_aspect() {
        let out = apply_resize(&img(1024, 512), &params(&[("width", "512")])).unwrap();
        assert_eq!((out.width(), out.height()), (512, 256));
    }

    #[test]
    fn resize_rejects_bad_fit_and_empty_box() {
        assert!(apply_resize(&img(64, 64), &params(&[("width", "10"), ("height", "10"), ("fit", "warp")])).is_err());
        assert!(apply_resize(&img(64, 64), &params(&[])).is_err());
    }

    #[test]
    fn apply_to_file_rewrites_png_in_place() {
        let dir = std::env::temp_dir().join(format!("media-tool-pp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("card.png");
        img(1024, 1024).save(&path).unwrap();

        let (w, h) = apply_to_file(&path, "crop", &params(&[("aspect_ratio", "1.91:1")])).unwrap();
        assert_eq!((w, h), (1024, 536));

        let (w, h) = apply_to_file(
            &path,
            "resize",
            &params(&[("width", "1200"), ("height", "630"), ("fit", "cover")]),
        )
        .unwrap();
        assert_eq!((w, h), (1200, 630));

        // Reopened from disk the file really is 1200x630.
        let reread = image::open(&path).unwrap();
        assert_eq!((reread.width(), reread.height()), (1200, 630));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_action_is_an_error() {
        let dir = std::env::temp_dir().join(format!("media-tool-pp-unk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.png");
        img(8, 8).save(&path).unwrap();
        assert!(apply_to_file(&path, "sharpen", &params(&[])).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handles_only_known_actions() {
        assert!(handles("crop"));
        assert!(handles("resize"));
        assert!(!handles("render"));
        assert!(!handles("sharpen"));
    }
}
