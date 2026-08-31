import Config

config :media_tool_web, MediaToolWeb.Endpoint,
  url: [host: "localhost"],
  adapter: Bandit.PhoenixAdapter,
  render_errors: [formats: [html: MediaToolWeb.ErrorHTML], layout: false],
  pubsub_server: MediaToolWeb.PubSub,
  live_view: [signing_salt: "mediaToolWeb"],
  secret_key_base: "wQ0nOc9uPjV4xYh7sLmZbA1eKd5GfJ6iHuWyQrXnMvBcTzS8oLpE0aG2u3Rk8tT2",
  http: [ip: {127, 0, 0, 1}, port: 8132],
  server: true

config :phoenix, :json_library, Jason

config :logger, level: :info
