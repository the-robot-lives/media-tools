import Config

# Test overrides — no HTTP listener; requests go through Phoenix.ConnTest.
config :media_tool_web, MediaToolWeb.Endpoint,
  server: false,
  http: [ip: {127, 0, 0, 1}, port: 8133]
