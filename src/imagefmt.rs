//! Identify image bytes by their magic number, and reconcile them with the extension a
//! prompt asked for.
//!
//! Providers do not always return the format they were asked for. The gemini image provider
//! returns JPEG whatever `output.format` a prompt declares, so `format: png` produced JPEG
//! bytes in a `.png` file, the run reported success, and post-processing then failed with
//! "cannot decode" — a silent-success shape, where the only honest signal arrived one step
//! too late.
//!
//! [`write_image_reconciled`] is the write path: sniff what actually came back and, when it
//! disagrees with the target extension, transcode to what was asked for. If the bytes cannot
//! be transcoded, write them under their true extension and say so, rather than leaving a
//! file whose name lies about its contents.

use std::path::{Path, PathBuf};

use color_eyre::eyre::WrapErr;

/// An image container we can recognise from its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    WebP,
    Gif,
}

impl ImageKind {
    /// The canonical file extension, without a dot.
    pub fn extension(self) -> &'static str {
        match self {
            ImageKind::Png => "png",
            ImageKind::Jpeg => "jpg",
            ImageKind::WebP => "webp",
            ImageKind::Gif => "gif",
        }
    }

    /// Does this extension name this container? Accepts the usual aliases.
    pub fn matches_extension(self, ext: &str) -> bool {
        let ext = ext.to_ascii_lowercase();
        match self {
            ImageKind::Png => ext == "png",
            ImageKind::Jpeg => ext == "jpg" || ext == "jpeg" || ext == "jpe",
            ImageKind::WebP => ext == "webp",
            ImageKind::Gif => ext == "gif",
        }
    }

    fn image_format(self) -> image::ImageFormat {
        match self {
            ImageKind::Png => image::ImageFormat::Png,
            ImageKind::Jpeg => image::ImageFormat::Jpeg,
            ImageKind::WebP => image::ImageFormat::WebP,
            ImageKind::Gif => image::ImageFormat::Gif,
        }
    }
}

/// Identify `bytes` by magic number, or `None` if it is not a container we know.
pub fn sniff(bytes: &[u8]) -> Option<ImageKind> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some(ImageKind::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageKind::Jpeg);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(ImageKind::WebP);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(ImageKind::Gif);
    }
    None
}

/// What happened when bytes were written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The bytes already matched the requested extension.
    AsRequested,
    /// The bytes were decoded and re-encoded into the requested format.
    Transcoded { from: ImageKind, to: ImageKind },
    /// The bytes could not be transcoded, so they were written under their true extension.
    Renamed { to: PathBuf, kind: ImageKind },
    /// The container was not recognised; bytes were written verbatim.
    Unknown,
}

/// Write image `bytes` to `output_path`, making the file's contents match its name.
///
/// Returns the path actually written and what had to be done to get there.
pub fn write_image_reconciled(
    output_path: &Path,
    bytes: &[u8],
) -> color_eyre::Result<(PathBuf, WriteOutcome)> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let requested_ext = output_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let actual = match sniff(bytes) {
        Some(kind) => kind,
        None => {
            std::fs::write(output_path, bytes)
                .wrap_err_with(|| format!("writing {}", output_path.display()))?;
            return Ok((output_path.to_path_buf(), WriteOutcome::Unknown));
        }
    };

    if actual.matches_extension(&requested_ext) {
        std::fs::write(output_path, bytes)
            .wrap_err_with(|| format!("writing {}", output_path.display()))?;
        return Ok((output_path.to_path_buf(), WriteOutcome::AsRequested));
    }

    // The prompt asked for a format the provider did not return. Prefer honouring the
    // request: decode what came back and re-encode as asked.
    let target = match requested_ext.as_str() {
        "png" => Some(ImageKind::Png),
        "jpg" | "jpeg" | "jpe" => Some(ImageKind::Jpeg),
        "webp" => Some(ImageKind::WebP),
        "gif" => Some(ImageKind::Gif),
        _ => None,
    };

    if let Some(target) = target {
        let decoded = image::load_from_memory_with_format(bytes, actual.image_format());
        if let Ok(img) = decoded {
            let mut encoded = Vec::new();
            // JPEG cannot carry alpha; drop it rather than fail the write.
            let to_encode = if target == ImageKind::Jpeg {
                image::DynamicImage::ImageRgb8(img.to_rgb8())
            } else {
                img
            };
            if to_encode
                .write_to(
                    &mut std::io::Cursor::new(&mut encoded),
                    target.image_format(),
                )
                .is_ok()
            {
                std::fs::write(output_path, &encoded)
                    .wrap_err_with(|| format!("writing {}", output_path.display()))?;
                return Ok((
                    output_path.to_path_buf(),
                    WriteOutcome::Transcoded {
                        from: actual,
                        to: target,
                    },
                ));
            }
        }
    }

    // Transcoding is not available for this pair. Never leave a file whose name lies about
    // its contents: write under the true extension and let the caller warn.
    let renamed = output_path.with_extension(actual.extension());
    std::fs::write(&renamed, bytes).wrap_err_with(|| format!("writing {}", renamed.display()))?;
    Ok((
        renamed.clone(),
        WriteOutcome::Renamed {
            to: renamed,
            kind: actual,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgba, RgbaImage};

    fn sample(format: image::ImageFormat) -> Vec<u8> {
        let mut buf = RgbaImage::new(64, 48);
        for (x, y, px) in buf.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 5) as u8, 90, 255]);
        }
        let img = if format == image::ImageFormat::Jpeg {
            DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(buf).to_rgb8())
        } else {
            DynamicImage::ImageRgba8(buf)
        };
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), format)
            .unwrap();
        out
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("media-tool-fmt-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sniffs_known_containers() {
        assert_eq!(sniff(&sample(image::ImageFormat::Png)), Some(ImageKind::Png));
        assert_eq!(sniff(&sample(image::ImageFormat::Jpeg)), Some(ImageKind::Jpeg));
        assert_eq!(sniff(&sample(image::ImageFormat::Gif)), Some(ImageKind::Gif));
        assert_eq!(sniff(b"GIF89a...."), Some(ImageKind::Gif));
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0, 0, 0, 0]);
        webp.extend_from_slice(b"WEBPVP8 ");
        assert_eq!(sniff(&webp), Some(ImageKind::WebP));
    }

    #[test]
    fn sniff_rejects_non_images_and_short_input() {
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"RIFF"), None);
        assert_eq!(sniff(b"<html>hello</html>"), None);
        assert_eq!(sniff(&[0x89, b'P']), None);
    }

    #[test]
    fn extension_aliases() {
        assert!(ImageKind::Jpeg.matches_extension("JPG"));
        assert!(ImageKind::Jpeg.matches_extension("jpeg"));
        assert!(ImageKind::Png.matches_extension("PNG"));
        assert!(!ImageKind::Png.matches_extension("jpg"));
    }

    #[test]
    fn matching_bytes_are_written_untouched() {
        let dir = tmpdir("match");
        let path = dir.join("a.png");
        let bytes = sample(image::ImageFormat::Png);
        let (written, outcome) = write_image_reconciled(&path, &bytes).unwrap();
        assert_eq!(outcome, WriteOutcome::AsRequested);
        assert_eq!(written, path);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The reported defect: a provider returns JPEG under a `png` declaration.
    #[test]
    fn jpeg_bytes_under_a_png_name_are_transcoded_to_png() {
        let dir = tmpdir("jpeg-as-png");
        let path = dir.join("card.png");
        let bytes = sample(image::ImageFormat::Jpeg);
        assert_eq!(sniff(&bytes), Some(ImageKind::Jpeg));

        let (written, outcome) = write_image_reconciled(&path, &bytes).unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::Transcoded {
                from: ImageKind::Jpeg,
                to: ImageKind::Png
            }
        );
        assert_eq!(written, path);

        // The file on disk is really a PNG now, and decodes.
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(sniff(&on_disk), Some(ImageKind::Png));
        let img = image::open(&path).expect("post-processing must be able to decode it");
        assert_eq!((img.width(), img.height()), (64, 48));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn png_bytes_under_a_jpg_name_are_transcoded_and_lose_alpha_cleanly() {
        let dir = tmpdir("png-as-jpg");
        let path = dir.join("card.jpg");
        let (_, outcome) = write_image_reconciled(&path, &sample(image::ImageFormat::Png)).unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::Transcoded {
                from: ImageKind::Png,
                to: ImageKind::Jpeg
            }
        );
        assert_eq!(sniff(&std::fs::read(&path).unwrap()), Some(ImageKind::Jpeg));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_bytes_are_written_verbatim() {
        let dir = tmpdir("unknown");
        let path = dir.join("thing.png");
        let (written, outcome) = write_image_reconciled(&path, b"<svg></svg>").unwrap();
        assert_eq!(outcome, WriteOutcome::Unknown);
        assert_eq!(written, path);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A container we recognise but cannot re-encode into the requested type must never leave
    /// a file whose name lies; it gets the true extension instead.
    #[test]
    fn unconvertible_target_extension_falls_back_to_the_true_extension() {
        let dir = tmpdir("rename");
        let path = dir.join("clip.mp4");
        let bytes = sample(image::ImageFormat::Png);
        let (written, outcome) = write_image_reconciled(&path, &bytes).unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::Renamed {
                to: dir.join("clip.png"),
                kind: ImageKind::Png
            }
        );
        assert_eq!(written, dir.join("clip.png"));
        assert!(written.exists());
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
