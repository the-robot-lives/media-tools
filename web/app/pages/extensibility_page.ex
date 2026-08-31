defmodule MediaToolWeb.Pages.ExtensibilityPage do
  @moduledoc "Extending media-tool — provider trait, runtime config, eval criteria, test lab."

  use Hologram.Page

  route "/extensibility"
  layout MediaToolWeb.Layouts.ExtensibilityLayout

  def init(_params, component, _server) do
    component
  end
end
