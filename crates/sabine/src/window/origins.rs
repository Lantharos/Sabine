//! The web origins whose pages may use the app's bridge.

use sabine_bridge::ContentSecurity;
use url::Url;

/// Lets pages served from `url`'s origin use the bridge.
pub(crate) fn allow_url_origin(security: &mut ContentSecurity, url: &str) {
    if let Some(url) = Url::parse(url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
    {
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

pub(crate) fn allow_origin(security: &mut ContentSecurity, origin: String) {
    security.remote_content = true;
    if !security
        .allowed_origins
        .iter()
        .any(|allowed| allowed == &origin)
    {
        security.allowed_origins.push(origin);
    }
}
