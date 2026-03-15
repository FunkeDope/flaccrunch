use crate::state::run_state::{FileEvent, FileStatus, RunSummary};
use crate::util::format::{format_bytes, format_elapsed, sha256_hex};
use std::time::Duration;

/// Generate the EFC-format final summary log text.
pub fn generate_efc_log(summary: &RunSummary, events: &[FileEvent]) -> String {
    let mut lines = vec![
        "═══════════════════════════════════════════════════════════".to_string(),
        "  FlacCrunch — Processing Summary".to_string(),
        "═══════════════════════════════════════════════════════════".to_string(),
        String::new(),
    ];

    // File results
    for event in events {
        let status = match event.status {
            FileStatus::OK => "OK  ",
            FileStatus::RETRY => "RTRY",
            FileStatus::FAIL => "FAIL",
        };
        lines.push(format!(
            "[{}] {} | {} | {} → {} | {} | {}",
            event.time,
            status,
            event.file,
            format_bytes(event.before_size as i64, false),
            format_bytes(event.after_size as i64, false),
            format_bytes(event.saved_bytes, true),
            event.verification,
        ));
    }

    lines.push(String::new());
    lines.push("───────────────────────────────────────────────────────────".to_string());
    lines.push(String::new());

    let c = &summary.counters;
    lines.push(format!("  Processed:    {}", c.processed));
    lines.push(format!("  Succeeded:    {}", c.successful));
    lines.push(format!("  Failed:       {}", c.failed));
    lines.push(format!(
        "  Pending:      {}",
        c.total_files - c.processed
    ));
    lines.push(format!(
        "  Elapsed:      {}",
        format_elapsed(Duration::from_secs(summary.elapsed_secs))
    ));
    lines.push(String::new());
    lines.push(format!(
        "  Total Saved:      {}",
        format_bytes(c.total_saved_bytes, true)
    ));
    lines.push(format!(
        "  Metadata Net:     {}",
        format_bytes(c.total_metadata_saved, true)
    ));
    lines.push(format!(
        "  Padding Trim:     {}",
        format_bytes(c.total_padding_saved, true)
    ));
    lines.push(format!(
        "  Artwork Net:      {}",
        format_bytes(c.total_artwork_saved, true)
    ));
    lines.push(format!(
        "  Artwork Raw:      {}",
        format_bytes(c.total_artwork_raw_saved, true)
    ));
    lines.push(format!(
        "  Artwork Files:    {}",
        c.artwork_optimized_files
    ));
    lines.push(format!(
        "  Artwork Blocks:   {}",
        c.artwork_optimized_blocks
    ));

    if c.successful > 0 {
        lines.push(String::new());
        let success_rate = (c.successful as f64 / c.total_files as f64) * 100.0;
        let avg_saved = c.total_saved_bytes / c.successful as i64;
        lines.push(format!("  Success Rate: {success_rate:.1}%"));
        lines.push(format!(
            "  Avg Saved:    {}",
            format_bytes(avg_saved, true)
        ));
    }

    // Top compressions
    if !summary.top_compression.is_empty() {
        lines.push(String::new());
        lines.push("  Top Compression:".to_string());
        for (i, tc) in summary.top_compression.iter().enumerate() {
            lines.push(format!(
                "    {}. {} ({}, {:.2}%)",
                i + 1,
                tc.path,
                format_bytes(tc.saved_bytes, true),
                tc.saved_pct
            ));
        }
    }

    lines.push(String::new());
    lines.push("═══════════════════════════════════════════════════════════".to_string());

    let body = lines.join("\n");
    let checksum = sha256_hex(&body);
    format!("{body}\n# SHA-256: {checksum}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::run_state::{CompressionResult, RunCounters};

    fn make_run_summary(total: usize, processed: usize, successful: usize, failed: usize) -> RunSummary {
        let counters = RunCounters {
            total_files: total,
            processed,
            successful,
            failed,
            ..RunCounters::default()
        };
        RunSummary {
            counters,
            elapsed_secs: 30,
            top_compression: vec![],
            status_lines: vec![],
        }
    }

    fn make_file_event(status: FileStatus, file: &str, detail: &str) -> FileEvent {
        FileEvent {
            time: "12:00:00".to_string(),
            status,
            file: file.to_string(),
            attempt: "1".to_string(),
            verification: "OK".to_string(),
            before_size: 1000,
            after_size: 900,
            saved_bytes: 100,
            compression_pct: 10.0,
            detail: detail.to_string(),
            source_hash: None,
            output_hash: None,
            embedded_md5: None,
            artwork_saved_bytes: 0,
            artwork_raw_saved_bytes: 0,
            artwork_blocks_optimized: 0,
        }
    }

    // --- generate_efc_log ---

    #[test]
    fn test_efc_log_contains_flaccrunch_header() {
        let summary = make_run_summary(0, 0, 0, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(
            output.contains("FlacCrunch"),
            "output must contain 'FlacCrunch', got: {output}"
        );
    }

    #[test]
    fn test_efc_log_contains_sha256_checksum() {
        let summary = make_run_summary(0, 0, 0, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(
            output.contains("# SHA-256:"),
            "output must contain SHA-256 line, got: {output}"
        );
    }

    #[test]
    fn test_efc_log_no_events_shows_counters() {
        let summary = make_run_summary(5, 5, 4, 1);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Succeeded:    4"), "must show successful count");
        assert!(output.contains("Failed:       1"), "must show failed count");
        assert!(output.contains("Processed:    5"), "must show processed count");
    }

    #[test]
    fn test_efc_log_includes_event_file_path() {
        let summary = make_run_summary(1, 1, 1, 0);
        let events = vec![make_file_event(FileStatus::OK, "/music/song.flac", "")];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("/music/song.flac"), "output must contain the file path");
    }

    #[test]
    fn test_efc_log_ok_status_label() {
        let summary = make_run_summary(1, 1, 1, 0);
        let events = vec![make_file_event(FileStatus::OK, "/music/ok.flac", "")];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("OK  "), "output must contain OK label");
    }

    #[test]
    fn test_efc_log_fail_status_label() {
        let summary = make_run_summary(1, 1, 0, 1);
        let events = vec![make_file_event(FileStatus::FAIL, "/music/fail.flac", "some error")];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("FAIL"), "output must contain FAIL label");
        assert!(output.contains("/music/fail.flac"), "output must contain file path");
    }

    #[test]
    fn test_efc_log_with_top_compression() {
        let mut summary = make_run_summary(1, 1, 1, 0);
        summary.top_compression = vec![CompressionResult {
            path: "/best.flac".to_string(),
            saved_bytes: 9000,
            saved_pct: 30.0,
            before_size: 30000,
            after_size: 21000,
        }];
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Top Compression"), "must include top compression section");
        assert!(output.contains("/best.flac"), "must include best file path");
    }

    #[test]
    fn test_efc_log_elapsed_appears() {
        let summary = make_run_summary(1, 1, 1, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Elapsed:"), "must include elapsed time");
    }

    #[test]
    fn test_efc_log_is_deterministic() {
        let summary = make_run_summary(2, 2, 2, 0);
        let events = vec![
            make_file_event(FileStatus::OK, "/a.flac", ""),
            make_file_event(FileStatus::OK, "/b.flac", ""),
        ];
        let out1 = generate_efc_log(&summary, &events);
        let out2 = generate_efc_log(&summary, &events);
        assert_eq!(out1, out2, "output must be deterministic");
    }

    // --- generate_failed_files_log ---

    #[test]
    fn test_failed_log_no_failures_returns_empty_string() {
        let events = vec![make_file_event(FileStatus::OK, "/ok.flac", "")];
        let output = generate_failed_files_log(&events);
        assert!(output.is_empty(), "expected empty string for no failures, got: {output}");
    }

    #[test]
    fn test_failed_log_empty_events_returns_empty_string() {
        let output = generate_failed_files_log(&[]);
        assert!(output.is_empty());
    }

    #[test]
    fn test_failed_log_contains_fail_file_path() {
        let events = vec![make_file_event(FileStatus::FAIL, "/music/broken.flac", "decode error")];
        let output = generate_failed_files_log(&events);
        assert!(output.contains("/music/broken.flac"), "output must contain the failed file path");
    }

    #[test]
    fn test_failed_log_contains_detail() {
        let events = vec![make_file_event(FileStatus::FAIL, "/bad.flac", "checksum mismatch")];
        let output = generate_failed_files_log(&events);
        assert!(output.contains("checksum mismatch"), "output must include detail/error");
    }

    #[test]
    fn test_failed_log_skips_ok_events() {
        let events = vec![
            make_file_event(FileStatus::OK, "/good.flac", ""),
            make_file_event(FileStatus::FAIL, "/bad.flac", "error"),
        ];
        let output = generate_failed_files_log(&events);
        assert!(output.contains("/bad.flac"), "must include bad.flac");
        assert!(!output.contains("/good.flac"), "must not include good.flac");
    }

    #[test]
    fn test_failed_log_multiple_failures() {
        let events = vec![
            make_file_event(FileStatus::FAIL, "/a.flac", "err a"),
            make_file_event(FileStatus::FAIL, "/b.flac", "err b"),
        ];
        let output = generate_failed_files_log(&events);
        assert!(output.contains("/a.flac"));
        assert!(output.contains("/b.flac"));
        assert!(output.contains("err a"));
        assert!(output.contains("err b"));
    }

    #[test]
    fn test_failed_log_has_header() {
        let events = vec![make_file_event(FileStatus::FAIL, "/x.flac", "oops")];
        let output = generate_failed_files_log(&events);
        assert!(output.contains("Failed Files"), "must include a header");
    }
}

/// Generate a failed files log.
pub fn generate_failed_files_log(events: &[FileEvent]) -> String {
    let failed: Vec<&FileEvent> = events
        .iter()
        .filter(|e| e.status == FileStatus::FAIL)
        .collect();

    if failed.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push("Failed Files Report".to_string());
    lines.push("===================".to_string());
    lines.push(String::new());

    for event in failed {
        lines.push(format!("File: {}", event.file));
        lines.push(format!("  Attempt: {}", event.attempt));
        lines.push(format!("  Detail: {}", event.detail));
        lines.push(String::new());
    }

    lines.join("\n")
}
