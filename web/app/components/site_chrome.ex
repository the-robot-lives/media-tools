defmodule MediaToolWeb.Components.SiteChrome do
  @moduledoc """
  Shared site chrome — sticky header nav and footer — wrapped around every
  page's content via its slot. Skins and tokens live in /assets/css/site.css.
  """

  use Hologram.Component

  def template do
    ~HOLO"""
    <a class="skip-link" href="#main">Skip to content</a>

    <header class="site-header">
      <div class="container header-inner">
        <a class="brand" href="/" aria-label="media-tool home">
          <span class="brand-mark" aria-hidden="true">◈</span>
          <span class="brand-name">media-tool</span>
        </a>
        <nav class="site-nav" aria-label="Primary">
          <a href="/format">The format</a>
          <a href="/providers">Providers</a>
          <a href="/extensibility">Extending</a>
          <a class="btn btn-primary btn-sm" href="/getting-started">Get started</a>
        </nav>
      </div>
    </header>

    <main id="main">
      <slot />
    </main>

    <footer class="site-footer">
      <div class="container footer-inner">
        <div class="footer-cols">
          <div class="footer-col">
            <p class="footer-brand"><span aria-hidden="true">◈</span> media-tool</p>
            <p class="footer-line">Media generation as versioned config. A Noizu Labs project.</p>
          </div>
          <div class="footer-col">
            <p class="footer-heading">Site</p>
            <a href="/format">The format</a>
            <a href="/providers">Providers</a>
            <a href="/extensibility">Extending</a>
            <a href="/getting-started">Getting started</a>
          </div>
          <div class="footer-col">
            <p class="footer-heading">Project</p>
            <a href="https://github.com/the-robot-lives/media-tool" rel="noopener">GitHub</a>
            <a href="https://github.com/the-robot-lives/media-tool/blob/main/LICENSE" rel="noopener">License</a>
            <a href="https://github.com/the-robot-lives/media-tool/blob/main/HOW-TO.md" rel="noopener">Prompt HOW-TO</a>
          </div>
        </div>
        <p class="footer-fine">© 2026 Noizu Labs · IO! Tessera License</p>
      </div>
    </footer>
    """
  end
end
