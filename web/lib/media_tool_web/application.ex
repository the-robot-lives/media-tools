defmodule MediaToolWeb.Application do
  use Application

  @impl true
  def start(_type, _args) do
    children = [
      {Phoenix.PubSub, name: MediaToolWeb.PubSub},
      MediaToolWeb.Endpoint
    ]

    opts = [strategy: :one_for_one, name: MediaToolWeb.Supervisor]
    Supervisor.start_link(children, opts)
  end
end
