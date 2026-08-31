defmodule MediaToolWeb.Layouts.FormatLayout do
  @moduledoc "Root layout for the the format page — per-page SEO head + shared chrome."

  use Hologram.Component

  def template do
    ~HOLO"""
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>The .media.prompt format — media-tool</title>
        <meta
          name="description"
          content="The .media.prompt file format (schema 0.4): declarative YAML for AI media assets — prompt text, quality tiers, attachments, dependency DAGs, post-processing, and eval criteria."
        />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="media-tool" />
        <meta property="og:title" content="The .media.prompt format — media-tool" />
        <meta
          property="og:description"
          content="The .media.prompt file format (schema 0.4): declarative YAML for AI media assets — prompt text, quality tiers, attachments, dependency DAGs, post-processing, and eval criteria."
        />
        <meta property="og:url" content="https://media-tool.therobotlives.com/format" />
        <link rel="icon" href="/assets/favicon.svg" type="image/svg+xml" />
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
        <link
          href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@500;600;700&amp;family=IBM+Plex+Mono:wght@400;500&amp;family=Inter:wght@400;500;600&amp;display=swap"
          rel="stylesheet"
        />
        <link rel="stylesheet" href="/assets/css/site.css" />
        <Hologram.UI.Runtime />
      </head>
      <body>
        <MediaToolWeb.Components.SiteChrome>
          <slot />
        </MediaToolWeb.Components.SiteChrome>
      </body>
    </html>
    """
  end
end
