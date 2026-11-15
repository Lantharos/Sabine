use std::time::{Duration, Instant};

use crate::RuntimeError;

const DEADLINE: Duration = Duration::from_secs(60);
const ATTEMPTS: u32 = 3;
const USER_AGENT: &str = concat!("Sabine/", env!("CARGO_PKG_VERSION"));

/// Fetches a document of at most `limit` bytes, retrying connection failures,
/// rate limits and server errors within a minute.
pub fn fetch_bytes(url: &str, limit: u64) -> Result<Vec<u8>, RuntimeError> {
    let deadline = Instant::now() + DEADLINE;
    let mut failure = format!("{url} timed out");
    for attempt in 0..ATTEMPTS {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match fetch_once(url, limit, remaining) {
            Ok(body) => return Ok(body),
            Err(Failure::Retry(message)) => failure = message,
            Err(failure) => return Err(failure.into()),
        }
        if attempt + 1 < ATTEMPTS {
            std::thread::sleep(
                Duration::from_secs(1 << attempt)
                    .min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
    Err(RuntimeError::Request(failure))
}

/// Fetches a document of at most `limit` bytes in one try that gives up after
/// `timeout`, for checks that mustn't hold anything up.
pub fn fetch_bytes_once(url: &str, limit: u64, timeout: Duration) -> Result<Vec<u8>, RuntimeError> {
    fetch_once(url, limit, timeout).map_err(RuntimeError::from)
}

enum Failure {
    Retry(String),
    Fatal(String),
    Gone(String),
}

impl From<Failure> for RuntimeError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Gone(message) => RuntimeError::Gone(message),
            Failure::Retry(message) | Failure::Fatal(message) => RuntimeError::Request(message),
        }
    }
}

fn fetch_once(url: &str, limit: u64, timeout: Duration) -> Result<Vec<u8>, Failure> {
    let mut response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .config()
        .https_only(true)
        .timeout_global(Some(timeout))
        .timeout_connect(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| match error {
            ureq::Error::RequireHttpsOnly(_) => Failure::Fatal(format!("{url} is not HTTPS")),
            error => Failure::Retry(format!("request to {url} failed: {error}")),
        })?;
    if let Some(failure) = status_failure(url, response.status().as_u16()) {
        return Err(failure);
    }
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => {
                Failure::Fatal(format!("{url} is larger than {limit} bytes"))
            }
            error => Failure::Retry(format!("reading {url} failed: {error}")),
        })
}

fn status_failure(url: &str, status: u16) -> Option<Failure> {
    match status {
        200 => None,
        410 => Some(Failure::Gone(format!("{url} is gone"))),
        429 | 500.. => Some(Failure::Retry(format!("{url} returned HTTP {status}"))),
        _ => Some(Failure::Fatal(format!("{url} returned HTTP {status}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_that_is_gone_is_told_apart_from_one_that_is_missing() {
        let url = "https://example.com/feed.json";
        assert!(status_failure(url, 200).is_none());
        assert!(matches!(
            status_failure(url, 410).map(RuntimeError::from),
            Some(RuntimeError::Gone(_))
        ));
        assert!(matches!(status_failure(url, 404), Some(Failure::Fatal(_))));
        assert!(matches!(status_failure(url, 503), Some(Failure::Retry(_))));
    }

    #[test]
    fn documents_are_only_fetched_over_https() {
        assert!(matches!(
            fetch_bytes("http://127.0.0.1:9/feed.json", 1024),
            Err(RuntimeError::Request(message)) if message.contains("not HTTPS")
        ));
    }
}
