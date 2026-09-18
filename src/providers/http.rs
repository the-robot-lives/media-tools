//! Shared `reqwest` client construction for providers.
//!
//! Providers used to call `reqwest::Client::new()` and then set a per-request
//! `.timeout(...)`. That leaves every *connection-level* default in place, so a long
//! render could be cut well before the per-request timeout: the connection pool may
//! retire an idle socket, and no TCP keepalive is sent while the client waits for
//! response headers, which lets a silent middlebox drop the flow. Building the client
//! explicitly puts those knobs where they can be seen and tuned.
//!
//! The per-request `.timeout()` calls elsewhere still apply; this only removes the
//! *hidden* ceilings beneath them.

use std::time::Duration;

/// Connect timeout for provider calls. Generous enough for TLS over a slow link, short
/// enough that a black-holed endpoint fails fast.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// TCP keepalive probe interval. Keeps the flow visible to NAT/firewall middleboxes
/// while the server is busy rendering and nothing is on the wire.
pub const TCP_KEEPALIVE: Duration = Duration::from_secs(15);

/// Build a client for a request expected to take up to `total`.
///
/// `pool_idle_timeout(None)` disables idle-connection retirement; `tcp_keepalive` keeps
/// a silent connection alive while a provider renders.
pub fn client_with_timeout(total: Duration) -> reqwest::Client {
    build(Some(total))
}

/// Build a client with no client-level timeout, for callers that set a per-request one.
pub fn client() -> reqwest::Client {
    build(None)
}

fn build(total: Option<Duration>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_idle_timeout(None)
        .tcp_keepalive(TCP_KEEPALIVE);

    if let Some(total) = total {
        builder = builder.timeout(total);
    }

    if std::env::var("MEDIA_DEBUG").ok().as_deref() == Some("1") {
        crate::telemetry::info(&format!(
            "http client: timeout={:?} connect_timeout={:?} pool_idle_timeout=disabled \
             tcp_keepalive={:?} http2=off (reqwest built with default-features = false)",
            total, CONNECT_TIMEOUT, TCP_KEEPALIVE
        ));
    }

    builder.build().unwrap_or_else(|e| {
        // A builder failure here means the TLS backend could not initialise; the plain
        // client would fail the same way on first use, so surface it there instead of
        // panicking during setup.
        crate::telemetry::warn_msg(&format!(
            "HTTP client builder failed ({e}); falling back to defaults"
        ));
        reqwest::Client::new()
    })
}

/// Read a positive-seconds override from an environment variable.
pub fn env_secs(name: &str) -> Option<Duration> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_secs_parses_and_rejects() {
        std::env::set_var("MEDIA_TEST_SECS_OK", "240");
        std::env::set_var("MEDIA_TEST_SECS_ZERO", "0");
        std::env::set_var("MEDIA_TEST_SECS_JUNK", "soon");
        assert_eq!(env_secs("MEDIA_TEST_SECS_OK"), Some(Duration::from_secs(240)));
        assert_eq!(env_secs("MEDIA_TEST_SECS_ZERO"), None);
        assert_eq!(env_secs("MEDIA_TEST_SECS_JUNK"), None);
        assert_eq!(env_secs("MEDIA_TEST_SECS_UNSET_XYZ"), None);
    }

    #[test]
    fn clients_build() {
        let _ = client();
        let _ = client_with_timeout(Duration::from_secs(240));
    }
}
