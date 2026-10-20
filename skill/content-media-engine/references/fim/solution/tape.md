# VHS — charmbracelet tape scripts (terminal GIFs as code)

VHS turns a `.tape` "movie script" into a GIF (or MP4/WebM) of a real terminal
session. The tape is a flat sequence of directives: style the screen, then
drive an actual shell with typed keystrokes. Rendered locally by the `vhs`
binary — no browser, no network.

**Current Version**: vhs 0.9.x  **License**: MIT  **Runtime**: `vhs` CLI (`go install github.com/charmbracelet/vhs@latest`); needs `ttyd` + `ffmpeg` at render time

## Core Syntax (Directives)

```tape
# 1. Where the render lands (first directive, one or more variants)
Output "demo.gif"
Output "demo.webm"

# 2. What the script needs (optional; copies assets next to the tape)
Require ./fixtures/example.json

# 3. Look & feel — the Settings (Typeset) block
Settings {
    FontFamily "JetBrains Mono"
    FontSize   22
    Theme      { "name": "Dracula" }
    Padding    60
    Width      1200
    Height     600
}

# 4. Behaviour — Set directives
Set Shell "zsh"
Set TypingSpeed 75ms      # default delay between typed characters
Set WaitTimeout 15s       # max wait for prompt between commands

# 5. The session itself
Type "ls -la"             # typed character by character
Enter
Sleep 1s
Type@500ms "cat README.md" Enter   # per-command speed override
Sleep 2s
Ctrl+C
```

Key/keystroke tokens: `Enter`, `Tab`, `Space`, `Backspace`, `Delete`,
`Insert`, `Ctrl+X`, `Alt+X`, `Up`/`Down`/`Left`/`Right`, `PageUp`/`PageDown`.
`Hide`/`Show` bracket commands that must run off-camera (setup, installs).

## Authoring rules for generated tapes

- **Output first.** A tape with no `Output` renders nothing useful; put the
  target filename in the first directive, matching the requested `.gif`.
- **One command per step.** Never collapse a session into a single `Type`
  line — the GIF must read as a demo, with a short `Sleep` after each
  command's output settles.
- **Pace for comprehension.** `Set TypingSpeed` (or `Type@`) plus 0.5–2s
  `Sleep`s; total runtime usually 10–30s.
- **Set the stage before typing.** `Settings` + `Set` blocks come before the
  session so the first frame is already styled; explicit `Width`/`Height`
  keeps the GIF crisp and shareable.
- **Keep the session honest.** The tape runs real commands — only reference
  binaries and files that exist in the render environment, and `Require`
  anything the script needs from disk.
- **Escape for the parser.** Shell quoting inside `Type "…"` follows the tape
  grammar; prefer single logical commands over chained one-liners when the
  chain is the point (`&&` chains render fine but read poorly at speed).
