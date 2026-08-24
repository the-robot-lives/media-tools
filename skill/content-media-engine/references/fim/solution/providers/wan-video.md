# Wan 2.7 — DashScope text/image-to-video

Use `service: wan-video` with model `wan2.7-t2v` (default) or `wan2.7-i2v` / `wan2.7-t2v-2026-06-12` / `happyhorse-1.1-t2v`.

Async: submit then poll `/api/v1/tasks/{id}`. Duration 2–15s, `ratio` 16:9, `resolution` 720P/1080P.

Auth: `DASHSCOPE_API_KEY`, `QWEN_API_KEY`, or `QWEN_TOKEN_KEY`. Token-plan host: `provider_options.plan: token-plan`.
