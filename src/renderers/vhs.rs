//! charmbracelet VHS renderer: `.tape` scripts → GIF (or the format named in
//! the tape's `Output` directive).
//!
//! Soft dependency: rendering needs the `vhs` binary on PATH. When absent the
//! renderer reports actionable guidance and skips — nothing else in the
//! pipeline hard-depends on it.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use super::Renderer;

pub struct VhsRenderer;

const INSTALL_HINT: &str =
    "install vhs: go install github.com/charmbracelet/vhs@latest (also needs ttyd + ffmpeg at render time)";

impl Renderer for VhsRenderer {
    fn render(
        &self,
        input_path: &Path,
        output_path: &Path,
        _params: &HashMap<String, serde_yaml::Value>,
    ) -> color_eyre::Result<bool> {
        if !self.is_available() {
            crate::telemetry::raw(&format!(
                "vhs renderer: 'vhs' not found on PATH — skipping. {}",
                INSTALL_HINT
            ));
            return Ok(false);
        }

        // vhs resolves the tape's relative `Output` path against its working
        // directory — run it where the caller expects the artifact to land,
        // then reconcile the produced file with the requested output path.
        let out_dir = output_path.parent().unwrap_or_else(|| Path::new("."));
        let output = Command::new("vhs")
            .arg(input_path)
            .current_dir(out_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            crate::telemetry::raw(&format!("vhs render failed: {}", stderr));
            return Ok(false);
        }

        // Absolute `Output` directive already landed on the requested path?
        if output_path.exists() {
            return Ok(true);
        }

        // Relative `Output`: move the freshest render vhs just wrote in the
        // working directory to the requested output name.
        if let Some(produced) = freshest_render(out_dir) {
            if produced != output_path {
                std::fs::rename(&produced, output_path)?;
            }
            return Ok(true);
        }

        crate::telemetry::raw(&format!(
            "vhs render produced no file for {} — check the tape's Output directive",
            output_path.display()
        ));
        Ok(false)
    }

    fn name(&self) -> &str {
        "vhs"
    }

    fn is_available(&self) -> bool {
        Command::new("vhs")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Freshest `.gif`/`.webm`/`.mp4` in `dir`, if any — the artifact vhs just wrote.
fn freshest_render(dir: &Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_render = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e, "gif" | "webm" | "mp4"))
            .unwrap_or(false);
        if !is_render {
            continue;
        }
        let Some(modified) = entry.metadata().and_then(|m| m.modified()).ok() else {
            continue;
        };
        if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshest_render_picks_gif_and_ignores_others() {
        let dir = std::env::temp_dir().join(format!("media-vhs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "not a render").unwrap();
        std::fs::write(dir.join("still.webp"), b"not a render either").unwrap();
        let gif = dir.join("demo.gif");
        std::fs::write(&gif, b"GIF89a").unwrap();

        // Only gif/webm/mp4 count; the freshest of those is picked (a single
        // render file keeps the mtime-ordering assertion deterministic).
        let got = freshest_render(&dir).expect("gif should be found");
        assert_eq!(got, gif);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn freshest_render_empty_dir_is_none() {
        let dir = std::env::temp_dir().join(format!("media-vhs-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(freshest_render(&dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
