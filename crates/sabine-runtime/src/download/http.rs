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
        .timeout_global(Some(timeout))
        .timeout_connect(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| Failure::Retry(format!("request to {url} failed: {error}")))?;
    let status = response.status().as_u16();
    if status == 429 || status >= 500 {
        return Err(Failure::Retry(format!("{url} returned HTTP {status}")));
    }
    if status == 410 {
        return Err(Failure::Gone(format!("{url} is gone")));
    }
    if status != 200 {
        return Err(Failure::Fatal(format!("{url} returned HTTP {status}")));
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

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;

    fn answering(status: &str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.read(&mut [0_u8; 1024]);
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });
        format!("http://{address}/feed.json")
    }

    #[test]
    fn a_document_that_is_gone_is_told_apart_from_one_that_is_missing() {
        let timeout = Duration::from_secs(5);
        assert!(matches!(
            fetch_bytes_once(&answering("410 Gone"), 1024, timeout),
            Err(RuntimeError::Gone(_))
        ));
        assert!(matches!(
            fetch_bytes_once(&answering("404 Not Found"), 1024, timeout),
            Err(RuntimeError::Request(_))
        ));
        assert!(matches!(
            fetch_bytes(&answering("410 Gone"), 1024),
            Err(RuntimeError::Gone(_))
        ));
    }
}
