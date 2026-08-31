defmodule MediaToolWeb.Pages.GettingStartedPage do
  @moduledoc "Getting started — install, first asset, dry-run, variants, refine, lab."

  use Hologram.Page

  route "/getting-started"
  layout MediaToolWeb.Layouts.GettingStartedLayout

  def init(_params, component, _server) do
    component
  end
end
