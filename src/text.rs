//! Char-boundary-safe string truncation.
//!
//! Everywhere the tool previews a string it does not control — an eval note, a provider error
//! body, an LLM's raw reply — it used to write `&s[..s.len().min(n)]`. That indexes *bytes*,
//! and Rust panics when the index lands inside a multibyte UTF-8 character. An eval note
//! containing an ellipsis or CJK text would take the whole run down while merely trying to log
//! itself.
//!
//! [`truncate`] cuts at the last character boundary at or before the limit instead.

/// Borrow at most `max_bytes` of `s`, cut at a character boundary.
///
/// Never panics, and never splits a character. The result may be shorter than `max_bytes`
/// when the boundary falls earlier.
pub fn truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Walk back to the last boundary at or before max_bytes. At most 3 steps for UTF-8.
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Like [`truncate`], but appends a single-character ellipsis when anything was cut.
pub fn truncate_ellipsis(s: &str, max_bytes: usize) -> String {
    let head = truncate(s, max_bytes);
    if head.len() == s.len() {
        head.to_string()
    } else {
        format!("{head}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_strings_pass_through() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("", 10), "");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn ascii_cuts_exactly() {
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("abcdef", 0), "");
    }

    /// The reported panic: an eval note containing an ellipsis, cut mid-character.
    #[test]
    fn ellipsis_note_does_not_panic() {
        // "…" is three bytes (E2 80 A6).
        let note = "score 0.62 \u{2026} composition drifts right";
        for limit in 0..note.len() + 5 {
            let out = truncate(note, limit);
            assert!(out.len() <= limit.min(note.len()));
            assert!(note.starts_with(out));
        }
        // Cutting one byte into the ellipsis yields the text before it.
        assert_eq!(truncate(note, 12), "score 0.62 ");
        assert_eq!(truncate(note, 13), "score 0.62 ");
        assert_eq!(truncate(note, 14), "score 0.62 \u{2026}");
    }

    #[test]
    fn cjk_is_never_split() {
        let note = "\u{8bc4}\u{4f30}\u{ff1a}\u{6784}\u{56fe}\u{504f}\u{79fb}";
        for limit in 0..note.len() + 5 {
            let out = truncate(note, limit);
            assert!(std::str::from_utf8(out.as_bytes()).is_ok());
            assert_eq!(out.len() % 3, 0, "each CJK char here is 3 bytes: {out:?}");
        }
        assert_eq!(truncate(note, 4), "\u{8bc4}");
        assert_eq!(truncate(note, 6), "\u{8bc4}\u{4f30}");
    }

    #[test]
    fn mixed_scripts_and_emoji() {
        // A 4-byte character, the widest UTF-8 gets.
        let s = "ok \u{1f680} go";
        for limit in 0..s.len() + 5 {
            let out = truncate(s, limit);
            assert!(s.starts_with(out));
        }
        assert_eq!(truncate(s, 4), "ok ");
        assert_eq!(truncate(s, 6), "ok ");
        assert_eq!(truncate(s, 7), "ok \u{1f680}");
    }

    #[test]
    fn ellipsis_marker_only_when_cut() {
        assert_eq!(truncate_ellipsis("abc", 10), "abc");
        assert_eq!(truncate_ellipsis("abcdef", 3), "abc\u{2026}");
        assert_eq!(truncate_ellipsis("\u{8bc4}\u{4f30}", 4), "\u{8bc4}\u{2026}");
    }

    /// Fuzz-ish sweep: no input and no limit may panic, and the result is always a prefix.
    #[test]
    fn never_panics_on_any_limit() {
        let samples = [
            "",
            "plain ascii",
            "\u{2026}\u{2026}\u{2026}",
            "\u{8bc4}\u{4f30}\u{ff1a}a\u{6784}",
            "\u{1f680}\u{1f680}",
            "caf\u{e9} na\u{ef}ve r\u{e9}sum\u{e9}",
        ];
        for s in samples {
            for limit in 0..s.len() + 8 {
                let out = truncate(s, limit);
                assert!(s.starts_with(out));
                let _ = truncate_ellipsis(s, limit);
            }
        }
    }
}
