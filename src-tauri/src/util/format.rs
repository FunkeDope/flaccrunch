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

/// Format a floating-point value as a right-aligned percentage string.
pub fn format_percent(value: f64) -> String {
    format!("{value:8.4}%")
}

/// Compute the SHA-256 hash of a text string, returning the hex digest.
pub fn sha256_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    hex_encode::encode(&result)
}

/// The null MD5 hash (all zeros) that FLAC uses when no MD5 is embedded.
pub const NULL_MD5: &str = "00000000000000000000000000000000";

/// Format a hash for display purposes.
pub fn format_hash_for_display(hash: &Option<String>) -> &str {
    match hash {
        Some(h) if h != NULL_MD5 => h.as_str(),
        Some(_) => "(null)",
        None => "(none)",
    }
}

// We need the hex crate for sha256_text; add it as a dependency or inline it
mod hex_encode {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(HEX_CHARS[(b >> 4) as usize] as char);
            s.push(HEX_CHARS[(b & 0x0f) as usize] as char);
        }
        s
    }
}

// Re-implement sha256_text without the hex crate dependency
pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    hex_encode::encode(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_zero() {
        assert_eq!(format_bytes(0, false), "0 B");
    }

    #[test]
    fn test_format_bytes_small() {
        assert_eq!(format_bytes(512, false), "512 B");
    }

    #[test]
    fn test_format_bytes_kilobytes() {
        assert_eq!(format_bytes(1536, false), "1.50 KB");
    }

    #[test]
    fn test_format_bytes_megabytes() {
        assert_eq!(format_bytes(1_572_864, false), "1.50 MB");
    }

    #[test]
    fn test_format_bytes_gigabytes() {
        assert_eq!(format_bytes(1_610_612_736, false), "1.50 GB");
    }

    #[test]
    fn test_format_bytes_signed_positive() {
        assert_eq!(format_bytes(1024, true), "+1.00 KB");
    }

    #[test]
    fn test_format_bytes_signed_negative() {
        assert_eq!(format_bytes(-2048, true), "-2.00 KB");
    }

    #[test]
    fn test_format_bytes_negative_unsigned() {
        assert_eq!(format_bytes(-512, false), "-512 B");
    }

    #[test]
    fn test_format_elapsed_simple() {
        let d = Duration::from_secs(3723); // 1h 2m 3s
        assert_eq!(format_elapsed(d), "01:02:03");
    }

    #[test]
    fn test_format_elapsed_zero() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "00:00:00");
    }

    #[test]
    fn test_format_elapsed_hours() {
        let d = Duration::from_secs(23 * 3600 + 59 * 60 + 59);
        assert_eq!(format_elapsed(d), "23:59:59");
    }

    #[test]
    fn test_format_elapsed_days() {
        let d = Duration::from_secs(2 * 86400 + 3 * 3600 + 15 * 60);
        assert_eq!(format_elapsed(d), "2d 03:15:00");
    }

    #[test]
    fn test_format_percent() {
        let s = format_percent(12.3456);
        assert!(s.contains("12.3456%"));
    }

    #[test]
    fn test_sha256_known_input() {
        let hash = sha256_hex("hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_format_hash_for_display_normal() {
        let h = Some("abcdef1234567890abcdef1234567890".to_string());
        assert_eq!(
            format_hash_for_display(&h),
            "abcdef1234567890abcdef1234567890"
        );
    }

    #[test]
    fn test_format_hash_for_display_null() {
        let h = Some(NULL_MD5.to_string());
        assert_eq!(format_hash_for_display(&h), "(null)");
    }

    #[test]
    fn test_format_hash_for_display_none() {
        assert_eq!(format_hash_for_display(&None), "(none)");
    }
}
