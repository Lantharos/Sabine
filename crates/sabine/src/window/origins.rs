//! The web origins whose pages may use the app's bridge: `https` origins, and
//! `http` ones only on the loopback interface, where nobody between the page
//! and the window can change it.

use sabine_bridge::ContentSecurity;
use url::{Host, Url};

use crate::window::config::SabineWindowConfig;
use crate::{SabineError, SabineResult};

/// Lets pages served from `url`'s origin use the bridge.
pub(crate) fn allow_url_origin(security: &mut ContentSecurity, url: &str) {
    if let Ok(url) = trusted_url(url) {
        allow_origin(security, url.origin().ascii_serialization());
    }
}

/// Lets a development server's pages use the bridge on every address the
/// server may answer on.
pub(crate) fn allow_dev_origins(security: &mut ContentSecurity, url: &str) {
    for candidate in crate::launch::dev_server::candidates(url) {
        allow_origin(security, candidate.origin().ascii_serialization());
    }
}

/// Lets `origin`'s pages use the bridge. An origin that isn't one the bridge
/// trusts stays as given, for [`check`] to refuse.
pub(crate) fn allow_origin(security: &mut ContentSecurity, origin: String) {
    let origin = trusted_origin(&origin).unwrap_or(origin);
    security.remote_content = true;
    if !security
        .allowed_origins
        .iter()
        .any(|allowed| allowed == &origin)
    {
        security.allowed_origins.push(origin);
    }
}

/// Refuses a production URL, an allowed origin or a command's origin that the
/// bridge can't trust.
pub(crate) fn check(config: &SabineWindowConfig) -> SabineResult<()> {
    let refused = |message: String| SabineError::CreationFailed { message };
    if let Some(url) = &config.url {
        trusted_url(url).map_err(refused)?;
    }
    let command_origins = config
        .bridge
        .descriptors()
        .iter()
        .flat_map(|descriptor| &descriptor.allowed_origins);
    for origin in config.security.allowed_origins.iter().chain(command_origins) {
        trusted_origin(origin).map_err(refused)?;
    }
    Ok(())
}

fn trusted_url(text: &str) -> Result<Url, String> {
    let url = Url::parse(text).map_err(|error| format!("`{text}` is not a URL: {error}"))?;
    let trusted = match url.scheme() {
        "https" => url.host_str().is_some_and(|host| !host.contains('*')),
        "http" => loopback(&url),
        _ => false,
    };
    if !trusted {
        return Err(format!(
            "`{text}` can't use the bridge: use https, or http only on localhost, 127.0.0.1 or [::1]"
        ));
    }
    Ok(url)
}

fn trusted_origin(text: &str) -> Result<String, String> {
    let url = trusted_url(text)?;
    let bare = matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    if !bare {
        return Err(format!(
            "`{text}` is not an origin: give only a scheme, host and port, such as https://example.com"
        ));
    }
    Ok(url.origin().ascii_serialization())
}

fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::trusted_origin;

    #[test]
    fn trusts_https_and_loopback_http_origins() {
        assert_eq!(
            trusted_origin("https://Accounts.Example.com/").unwrap(),
            "https://accounts.example.com"
        );
        assert_eq!(
            trusted_origin("http://localhost:5173").unwrap(),
            "http://localhost:5173"
        );
        assert_eq!(trusted_origin("http://[::1]:8080").unwrap(), "http://[::1]:8080");
    }

    #[test]
    fn refuses_origins_anyone_on_the_network_could_serve() {
        for origin in [
            "*",
            "http://example.com",
            "https://*.example.com",
            "https://example.com/app",
            "https://example.com?next=1",
            "file:///home",
            "data:text/html,hi",
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop",
        ] {
            assert!(trusted_origin(origin).is_err(), "{origin}");
        }
    }
}
