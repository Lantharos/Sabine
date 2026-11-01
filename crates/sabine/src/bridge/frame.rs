//! Bridge messages between the app, its windows and the browser host: one
//! line each, which may carry raw bytes after it.

use std::{
    io::{self, BufRead, Read},
    sync::Arc,
};

/// Starts a line whose message carries bytes: `SABINE_BRIDGE_BYTES\t<length>\t<message>`,
/// with `<length>` bytes following its newline.
pub(crate) const BYTES_PREFIX: &str = "SABINE_BRIDGE_BYTES\t";
pub(crate) const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) line: String,
    pub(crate) body: Option<Vec<u8>>,
}

impl Frame {
    pub(crate) fn line(line: impl Into<String>) -> Self {
        Self {
            line: line.into(),
            body: None,
        }
    }

    pub(crate) fn encode(line: &str, body: Option<&[u8]>) -> Vec<u8> {
        with_parts(line, body, |parts| parts.concat())
    }

    /// Encodes a message straight into the buffer every window it goes to
    /// shares, copying its line and bytes once.
    pub(crate) fn shared(line: &str, body: Option<&[u8]>) -> Arc<[u8]> {
        with_parts(line, body, |parts| {
            let mut frame =
                Arc::<[u8]>::new_uninit_slice(parts.iter().map(|part| part.len()).sum());
            let mut free = Arc::get_mut(&mut frame).expect("a new frame has no other owner");
            for part in parts {
                let (filled, rest) = free.split_at_mut(part.len());
                filled.write_copy_of_slice(part);
                free = rest;
            }
            unsafe { frame.assume_init() }
        })
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        Self::encode(&self.line, self.body.as_deref())
    }

    /// Reads the next message from a stream, or `None` at its end.
    pub(crate) fn read(reader: &mut impl BufRead) -> io::Result<Option<Self>> {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\n', '\r']);
        let Some((length, message)) = split_bytes_header(line)? else {
            return Ok(Some(Self::line(line)));
        };
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        Ok(Some(Self {
            line: message.to_string(),
            body: Some(body),
        }))
    }

    /// Decodes one message whose line and bytes arrived together.
    pub(crate) fn decode(bytes: &[u8]) -> io::Result<Self> {
        let newline = bytes.iter().position(|byte| *byte == b'\n');
        let (line, rest) = match newline {
            Some(newline) => (&bytes[..newline], &bytes[newline + 1..]),
            None => (bytes, &[][..]),
        };
        let line = std::str::from_utf8(line).map_err(invalid)?;
        let Some((length, message)) = split_bytes_header(line)? else {
            return Ok(Self::line(line));
        };
        let mut body = Vec::with_capacity(length);
        rest.take(length as u64).read_to_end(&mut body)?;
        if body.len() != length {
            return Err(invalid("a bridge message ended before its bytes"));
        }
        Ok(Self {
            line: message.to_string(),
            body: Some(body),
        })
    }
}

/// The pieces of a message in order: the bytes header when it carries
/// bytes, its line, the newline and its bytes.
fn with_parts<T>(line: &str, body: Option<&[u8]>, encode: impl FnOnce([&[u8]; 4]) -> T) -> T {
    let header = body
        .map(|body| format!("{BYTES_PREFIX}{}\t", body.len()))
        .unwrap_or_default();
    encode([
        header.as_bytes(),
        line.as_bytes(),
        b"\n",
        body.unwrap_or_default(),
    ])
}

fn split_bytes_header(line: &str) -> io::Result<Option<(usize, &str)>> {
    let Some(header) = line.strip_prefix(BYTES_PREFIX) else {
        return Ok(None);
    };
    let (length, message) = header
        .split_once('\t')
        .ok_or_else(|| invalid("a bridge message is missing its length"))?;
    let length = length.parse::<usize>().map_err(invalid)?;
    if length > MAX_BODY_BYTES {
        return Err(invalid("a bridge message carries more than 32 MiB"));
    }
    Ok(Some((length, message)))
}

fn invalid(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_bytes_follow_each_other_in_a_stream() {
        let mut stream = Vec::new();
        stream.extend(Frame::encode("SABINE_BRIDGE_EVENT\t\"a\"\tnull", None));
        stream.extend(Frame::encode(
            "SABINE_BRIDGE_EVENT\t\"b\"\tnull",
            Some(b"\n\x00\xff"),
        ));
        stream.extend(Frame::encode("SABINE_OSR_READY", None));
        let mut reader = stream.as_slice();
        let frames = std::iter::from_fn(|| Frame::read(&mut reader).unwrap()).collect::<Vec<_>>();
        assert_eq!(
            frames,
            [
                Frame::line("SABINE_BRIDGE_EVENT\t\"a\"\tnull"),
                Frame {
                    line: "SABINE_BRIDGE_EVENT\t\"b\"\tnull".to_string(),
                    body: Some(b"\n\x00\xff".to_vec()),
                },
                Frame::line("SABINE_OSR_READY"),
            ]
        );
    }

    #[test]
    fn a_message_in_one_piece_decodes_with_its_bytes() {
        let bytes = Frame::encode("SABINE_BRIDGE_REQUEST\t1\t2\t\tsave\t{}", Some(b"data"));
        let frame = Frame::decode(&bytes).unwrap();
        assert_eq!(frame.line, "SABINE_BRIDGE_REQUEST\t1\t2\t\tsave\t{}");
        assert_eq!(frame.body.as_deref(), Some(b"data".as_slice()));
    }

    #[test]
    fn shared_frames_match_encoded_ones() {
        let line = "SABINE_BRIDGE_EVENT\t\"a\"\tnull";
        assert_eq!(*Frame::shared(line, None), *Frame::encode(line, None));
        assert_eq!(
            *Frame::shared(line, Some(b"data")),
            *Frame::encode(line, Some(b"data"))
        );
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let mut bytes = Frame::encode("SABINE_BRIDGE_EVENT\t\"a\"\tnull", Some(b"data"));
        bytes.pop();
        assert!(Frame::decode(&bytes).is_err());
    }
}
