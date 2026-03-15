use serde::{Deserialize, Serialize};

/// Parsed progress information from flac's stderr output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlacProgress {
    pub percent: u8,
    pub ratio: String,
}

/// Parse flac encoding progress from stderr content.
/// Looks for lines like "filename: 55% complete, ratio=0.652"
pub fn parse_flac_progress(stderr_content: &str) -> FlacProgress {
    let mut last_percent: u8 = 0;
    let mut last_ratio = String::new();

    for line in stderr_content.lines().rev() {
        if let Some(pct_idx) = line.find("% complete") {
            // Find the percentage number before "% complete"
            let before = &line[..pct_idx];
            if let Some(pct_str) = before.split_whitespace().next_back() {
                // Handle case where it might be "55" or part of a longer string like "file: 55"
                let digits: String = pct_str.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(pct) = digits.parse::<u8>() {
                    last_percent = pct;
                }
            }

            // Find ratio
            if let Some(ratio_idx) = line.find("ratio=") {
                let ratio_str = &line[ratio_idx + 6..];
                let ratio_end = ratio_str
                    .find(|c: char| !c.is_ascii_digit() && c != '.')
                    .unwrap_or(ratio_str.len());
                last_ratio = ratio_str[..ratio_end].to_string();
            }

            break; // Found the last progress line
        }
    }

    FlacProgress {
        percent: last_percent,
        ratio: last_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flac_progress_normal() {
        let stderr = "track.flac: 55% complete, ratio=0.652\n";
        let progress = parse_flac_progress(stderr);
        assert_eq!(progress.percent, 55);
        assert_eq!(progress.ratio, "0.652");
    }

    #[test]
    fn test_parse_flac_progress_complete() {
        let stderr = "track.flac: 100% complete, ratio=0.612\n";
        let progress = parse_flac_progress(stderr);
        assert_eq!(progress.percent, 100);
        assert_eq!(progress.ratio, "0.612");
    }

    #[test]
    fn test_parse_flac_progress_empty() {
        let progress = parse_flac_progress("");
        assert_eq!(progress.percent, 0);
        assert_eq!(progress.ratio, "");
    }

    #[test]
    fn test_parse_flac_progress_multiple_lines() {
        let stderr = "track.flac: 10% complete, ratio=0.800\n\
                       track.flac: 50% complete, ratio=0.700\n\
                       track.flac: 90% complete, ratio=0.650\n";
        let progress = parse_flac_progress(stderr);
        assert_eq!(progress.percent, 90);
        assert_eq!(progress.ratio, "0.650");
    }

    #[test]
    fn test_parse_flac_progress_no_ratio() {
        let stderr = "track.flac: 30% complete\n";
        let progress = parse_flac_progress(stderr);
        assert_eq!(progress.percent, 30);
        assert_eq!(progress.ratio, "");
    }
}
