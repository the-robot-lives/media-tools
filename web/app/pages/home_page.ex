defmodule MediaToolWeb.Pages.HomePage do
  @moduledoc """
  Landing page — hero, benefits, format-at-a-glance, asset types, pipeline fit.
  Static SSR; no client state or actions needed.
  """

  use Hologram.Page

  route "/"
  layout MediaToolWeb.Layouts.HomeLayout

  def init(_params, component, _server) do
    component
  end
end
