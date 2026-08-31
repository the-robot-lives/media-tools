defmodule MediaToolWeb.Pages.FormatPage do
  @moduledoc "The .media.prompt format — schema walkthrough, dependencies, eval."

  use Hologram.Page

  route "/format"
  layout MediaToolWeb.Layouts.FormatLayout

  def init(_params, component, _server) do
    component
  end
end
