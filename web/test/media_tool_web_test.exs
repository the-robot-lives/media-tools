defmodule MediaToolWeb.RequestTest do
  use ExUnit.Case, async: false
  import Phoenix.ConnTest

  @endpoint MediaToolWeb.Endpoint

  test "GET / renders the home page hero" do
    conn = get(build_conn(), "/")
    assert conn.status == 200
    assert conn.resp_body =~ "Media generation you can commit."
    assert conn.resp_body =~ "generate-media-prompt"
  end

  test "all pages render with per-page titles" do
    assert get(build_conn(), "/format") |> Map.get(:status) == 200
    assert get(build_conn(), "/format").resp_body =~ "The .media.prompt format"

    assert get(build_conn(), "/providers").resp_body =~ "Provider matrix"
    assert get(build_conn(), "/extensibility").resp_body =~ "Extend everything"
    assert get(build_conn(), "/getting-started").resp_body =~ "From zero to generated"
  end

  test "static assets are served" do
    conn = get(build_conn(), "/assets/css/site.css")
    assert conn.status == 200

    conn = get(build_conn(), "/assets/favicon.svg")
    assert conn.status == 200
  end

  test "unknown route returns 404" do
    conn = get(build_conn(), "/nope")
    assert conn.status == 404
  end
end
