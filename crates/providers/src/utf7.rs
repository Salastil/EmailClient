//! IMAP modified UTF-7 (RFC 3501 §5.1.3) for mailbox names.

use base64::alphabet::IMAP_MUTF7;
use base64::engine::general_purpose::{GeneralPurpose, NO_PAD};
use base64::Engine;

const ENGINE: GeneralPurpose = GeneralPurpose::new(&IMAP_MUTF7, NO_PAD);

pub fn decode(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let chunk = &after[..end];
        if chunk.is_empty() {
            out.push('&');
        } else if let Ok(bytes) = ENGINE.decode(chunk) {
            let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            out.push_str(&String::from_utf16_lossy(&units));
        } else {
            out.push('&');
            out.push_str(chunk);
            out.push('-');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn encode(s: &str) -> String {
    let mut out = String::new();
    let mut pending: Vec<u16> = Vec::new();
    let flush = |pending: &mut Vec<u16>, out: &mut String| {
        if !pending.is_empty() {
            let bytes: Vec<u8> = pending.iter().flat_map(|u| u.to_be_bytes()).collect();
            out.push('&');
            out.push_str(&ENGINE.encode(bytes));
            out.push('-');
            pending.clear();
        }
    };
    for c in s.chars() {
        if (' '..='~').contains(&c) {
            flush(&mut pending, &mut out);
            if c == '&' {
                out.push_str("&-");
            } else {
                out.push(c);
            }
        } else {
            let mut buf = [0u16; 2];
            pending.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    flush(&mut pending, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for (plain, wire) in [
            ("Entwürfe", "Entw&APw-rfe"),
            ("Tom & Jerry", "Tom &- Jerry"),
            ("台北", "&U,BTFw-"),
            ("INBOX", "INBOX"),
        ] {
            assert_eq!(encode(plain), wire);
            assert_eq!(decode(wire), plain);
        }
    }
}
