defmodule MediaToolWeb.Endpoint do
  use Phoenix.Endpoint, otp_app: :media_tool_web

  # Hologram's client runtime is served from the "hologram" path;
  # our own assets from "assets" and "images".
  plug Plug.Static,
    at: "/",
    from: :media_tool_web,
    only: ["assets", "images", "hologram"],
    gzip: false

  plug Plug.Parsers,
    parsers: [:urlencoded, :multipart, :json],
    pass: ["*/*"],
    json_decoder: Jason

  plug Plug.Session,
    store: :cookie,
    key: "_media_tool_web_key",
    signing_salt: "mediaToolWeb"

  # Hologram's router must run BEFORE the Phoenix router.
  plug Hologram.Router
  plug MediaToolWeb.Router
end
