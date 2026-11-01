//! A development server and the addresses it may answer on.

use std::{
    net::TcpStream,
    thread,
    time::{Duration, Instant},
};

use url::{Host, Url};

use crate::{SabineError, SabineResult};

const TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(150);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

/// The URLs a development server at `url` may answer on: `url` itself and,
/// for a loopback or unspecified host, the same URL on every loopback name.
pub(crate) fn candidates(url: &str) -> Vec<Url> {
    let Some(url) = Url::parse(url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
    else {
        return Vec::new();
    };
    let local = match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback() || address.is_unspecified(),
        Some(Host::Ipv6(address)) => address.is_loopback() || address.is_unspecified(),
        None => false,
    };
    let mut candidates = vec![url.clone()];
    if local {
        for host in LOOPBACK_HOSTS {
            let mut candidate = url.clone();
            if candidate.set_host(Some(host)).is_ok() && !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

/// The port the development server at `url` listens on.
#[cfg(target_os = "linux")]
pub(crate) fn port(url: &str) -> Option<u16> {
    candidates(url).first()?.port_or_known_default()
}

/// Waits for the development server at `url` to accept connections and
/// returns the URL it answered on.
pub(crate) fn wait(url: &str) -> SabineResult<String> {
    let candidates = candidates(url);
    if candidates.is_empty() {
        return Ok(url.to_string());
    }
    let deadline = Instant::now() + TIMEOUT;
    let mut last_error = None;
    while Instant::now() < deadline {
        for candidate in &candidates {
            match candidate.socket_addrs(|| None) {
                Ok(addresses) => {
                    for address in addresses {
                        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                            Ok(_) => return Ok(candidate.to_string()),
                            Err(error) => last_error = Some(error),
                        }
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }
        thread::sleep(RETRY_INTERVAL);
    }
    Err(SabineError::CreationFailed {
        message: format!(
            "timed out waiting for dev server `{url}`{}",
            last_error
                .map(|error| format!(": {error}"))
                .unwrap_or_default()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::candidates;

    #[test]
    fn loopback_servers_answer_on_every_loopback_name() {
        let urls = candidates("http://localhost:5173/app?chrome=app")
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        assert_eq!(
            urls,
            [
                "http://localhost:5173/app?chrome=app",
                "http://127.0.0.1:5173/app?chrome=app",
                "http://[::1]:5173/app?chrome=app",
            ]
        );
        assert_eq!(candidates("https://example.com").len(), 1);
        assert!(candidates("sabine://app/index.html").is_empty());
    }
}
