use sha2::{Digest, Sha256};
use std::time::Duration;

/// Format a byte count into a human-readable string (e.g., "1.23 MB").
/// If `signed` is true, positive values get a "+" prefix.
pub fn format_bytes(bytes: i64, signed: bool) -> String {
    let abs = bytes.unsigned_abs();
    let sign = if bytes < 0 {
        "-"
    } else if signed {
        "+"
    } else {
        ""
    };

    if abs < 1024 {
        format!("{sign}{abs} B")
    } else if abs < 1024 * 1024 {
        format!("{sign}{:.2} KB", abs as f64 / 1024.0)
    } else if abs < 1024 * 1024 * 1024 {
        format!("{sign}{:.2} MB", abs as f64 / (1024.0 * 1024.0))
    } else {
        format!("{sign}{:.2} GB", abs as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Format a Duration into "HH:MM:SS" or "Xd HH:MM:SS" for durations over 24h.
pub fn format_elapsed(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours >= 24 {
        let days = hours / 24;
        let remaining_hours = hours % 24;
        format!("{days}d {remaining_hours:02}:{mins:02}:{secs:02}")
    } else {
        format!("{hours:02}:{mins:02}:{secs:02}")
    }
}

/// The null MD5 hash (all zeros) that FLAC uses when no MD5 is embedded.
pub const NULL_MD5: &str = "00000000000000000000000000000000";

/// Compute the SHA-256 hash of a text string, returning the lowercase hex digest.
pub fn sha256_hex(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(text.as_bytes());
    let mut s = String::with_capacity(digest.len() * 2);
    for &b in digest.iter() {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0, false), "0 B");
        assert_eq!(format_bytes(512, false), "512 B");
        assert_eq!(format_bytes(1536, false), "1.50 KB");
        assert_eq!(format_bytes(1_572_864, false), "1.50 MB");
        assert_eq!(format_bytes(1_610_612_736, false), "1.50 GB");
        assert_eq!(format_bytes(1024, true), "+1.00 KB");
        assert_eq!(format_bytes(-2048, true), "-2.00 KB");
        assert_eq!(format_bytes(-512, false), "-512 B");
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "00:00:00");
        assert_eq!(format_elapsed(Duration::from_secs(3723)), "01:02:03");
        assert_eq!(
            format_elapsed(Duration::from_secs(23 * 3600 + 59 * 60 + 59)),
            "23:59:59"
        );
        assert_eq!(
            format_elapsed(Duration::from_secs(2 * 86400 + 3 * 3600 + 15 * 60)),
            "2d 03:15:00"
        );
    }

    #[test]
    fn test_sha256_known_input() {
        assert_eq!(
            sha256_hex("hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
