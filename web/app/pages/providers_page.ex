defmodule MediaToolWeb.Pages.ProvidersPage do
  @moduledoc "Provider matrix — implemented services, quality-tier ladders, provider options."

  use Hologram.Page

  route "/providers"
  layout MediaToolWeb.Layouts.ProvidersLayout

  def init(_params, component, _server) do
    component
  end
end
