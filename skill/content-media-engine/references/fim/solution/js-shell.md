# SemTextShell JS shell packs (ES-module command plugins)

A JS shell pack is an ES module that extends the SemTextShell terminal
(`<sem-shell>` / `<sem-terminal>`) with named commands. The module's default
export is an array of `CommandSpec` objects; the terminal mounts them, shows
them in `help`, and routes parsed input lines to their `run(ctx)`.

**Current Version**: SemTextShell plugin contract (semtext.dev npl-enhanced-content, src/terminal/PLUGINS.md)  **Runtime**: any ES-module host; no build step, no framework

## Pack shape

```js
// my-pack.mjs — default-exports CommandSpec[]
const greet = {
  name: "greet",                    // /^[a-z][a-z0-9-]*$/, unique per terminal
  usage: "greet [-n] <who>",        // the `help` line
  run(ctx) {
    const loud = !ctx.args.includes("-n");
    const who = ctx.args.filter((a) => a !== "-n")[0] ?? "world";
    ctx.out(loud ? who.toUpperCase() + "!" : who);
  },
};

export default [greet];
```

A JSON manifest form wraps the same module for CDN distribution:

```jsonc
{
  "name": "my-pack",
  "version": "0.1.0",
  "load": { "src": "./commands.js", "integrity": "sha256-<64 hex>" },
  "commands": [],
  "files": [{ "path": "/my-pack/welcome.txt", "src": "files/welcome.txt" }]
}
```

## CommandContext

```ts
interface CommandContext {
  args: string[];          // argv after the command name (expanded)
  stdin: string | null;    // piped text from the previous stage, else null
  vfs: Vfs;                // the terminal's virtual file system
  state: ShellState;       // cwd, env, aliases, history, lastStatus
  color: boolean;          // true when stdout is the live terminal (ANSI ok)
  commands: ReadonlyMap;   // every registered command
  out(text: string): void; // one stdout line (also piped stdout)
  err(text: string): void; // one stderr line — errors print, never throw
  emit(action): void;      // side-effect requests, DATA only
  exit(code): void;        // set exit status
}
```

## Authoring rules for generated packs

- **Default export is the contract.** Exactly one default export: the
  `CommandSpec[]` array. No named exports required, no side module imports
  beyond the standard library.
- **No side effects at import time.** No `fetch`, no DOM, no timers, no
  top-level mutation. Importing the pack must be free; all behaviour lives
  inside `run(ctx)`.
- **Names are unique and lowercase.** `name` matches `/^[a-z][a-z0-9-]*$/`;
  a duplicate name fails the whole plugin mount, so avoid colliding with
  builtins (`help`, `clear`) and the POSIX set (`ls`, `cat`, `cd`, `grep`, …).
- **Write through `ctx.out` / `ctx.err`.** Piping (`my-cmd | grep x`) and
  redirects capture exactly what `out` emits; report failures on `err` and
  set a non-zero status with `ctx.exit(code)` instead of throwing.
- **Honor `ctx.stdin` and `ctx.color`.** Read piped input when present; emit
  ANSI color only when `ctx.color` is true.
- **Files ride under your own prefix.** Mounted documents must use absolute
  paths (`/my-pack/...`) so packs never step on each other.
- **Integrity is expected.** Remote (absolute https) loading requires a
  `sha256-<64 hex>` hash over the exact bytes; keep packs as plain modules so
  no hash gymnastics are needed in the common case.
