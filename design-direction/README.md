# Visual-direction mockups — media-tool landing site

Four `.media.prompt` candidates for the site hero / OG art. **Not generated yet** —
the user picks a direction (or asks for revisions) and the critique loop runs after.

Model notes (from the repo's runtime YAML, `media-tool.yaml`):
- current image models: `gemini:gemini-3.1-flash-image` (default/medium),
  `gemini:gemini-3-pro-image` (high/premium), `qwen-image:qwen-image-3.0` fallback
- predict endpoint is dead; generateContent is current (gemini.rs routes there)
- if the installed `generate-media-prompt` binary silently fails, rebuild:
  `cd <repo> && make install` (stale-binary footgun)

Generate the chosen one with:
```bash
generate-media-prompt -n 3 design-direction/01-<slug>.media.prompt
```

| # | File | Direction | Mood |
|---|------|-----------|------|
| 1 | `01-terminal-tessera.media.prompt` | Code-as-hero: floating glass YAML panel | dark, technical, confident |
| 2 | `02-blueprint-dag.media.prompt` | Engineering blueprint of the dependency DAG | navy, schematic, precise |
| 3 | `03-gradient-foundry.media.prompt` | Assets on a foundry assembly line | warm, generative, energetic |
| 4 | `04-quiet-paper.media.prompt` | Swiss-minimal light mode, one card + mark | light, editorial, calm |
