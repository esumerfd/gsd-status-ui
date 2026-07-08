//! Terminal clipboard via OSC 52. Dependency-free and SSH-friendly: the
//! sequence tells the *terminal* to set the system clipboard, so it works
//! wherever the terminal supports it (iTerm2, kitty, wezterm, tmux, …)
//! without linking a platform clipboard library.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 (with `=` padding). Small hand-rolled encoder so the crate
/// stays dependency-free for the one place OSC 52 needs it.
pub(crate) fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// The OSC 52 "set clipboard" sequence for `text`, targeting the `c` (clipboard)
/// selection and terminated with BEL. Write it straight to the terminal.
pub(crate) fn osc52_copy_sequence(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64_encode(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"hi"), "aGk=");
        assert_eq!(base64_encode(b"Man"), "TWFu");
    }

    #[test]
    fn osc52_wraps_base64_in_the_clipboard_sequence() {
        assert_eq!(osc52_copy_sequence("hi"), "\x1b]52;c;aGk=\x07");
    }
}
