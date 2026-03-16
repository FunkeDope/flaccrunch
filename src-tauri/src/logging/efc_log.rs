use crate::state::run_state::{FileEvent, FileStatus, RunSummary};
use crate::util::format::sha256_hex;
use chrono::{Local, TimeZone};

/// Format a value line in EAC style:
///   `     {label:<18} {value}`
fn eac_line(label: &str, value: &str) -> String {
    format!("     {:<18} {}", label, value)
}

/// Format bytes like PS Format-Bytes (unsigned): `"   01.50 KB"` (8-char right-aligned number).
fn fmt_bytes(bytes: u64) -> String {
    let v = bytes as f64;
    let (val, unit) = if v >= 1_073_741_824.0 {
        (v / 1_073_741_824.0, "GB")
    } else if v >= 1_048_576.0 {
        (v / 1_048_576.0, "MB")
    } else if v >= 1024.0 {
        (v / 1024.0, "KB")
    } else {
        (v, "B")
    };
    format!("{:>8} {}", format!("{:05.2}", val), unit)
}

/// Format a Unix-ms timestamp as EAC log datetime: `"15. March 2026, 14:30"`.
fn fmt_eac_datetime(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            // %-d = day without leading zero, %B = full month, %-H = hour without leading zero
            let day = dt.format("%d").to_string();
            let day = day.trim_start_matches('0');
            format!("{}. {}", day, dt.format("%B %Y, %-H:%M"))
        }
        chrono::LocalResult::None => ms.to_string(),
    }
}

/// Generate the EFC-format final summary log text, matching the PowerShell
/// `New-EfcFinalLogText` output exactly.
pub fn generate_efc_log(summary: &RunSummary, events: &[FileEvent]) -> String {
    let mut lines: Vec<String> = Vec::new();

    // ---- Header ----
    lines.push("Exact Flac Cruncher".to_string());
    lines.push(String::new());
    lines.push(format!(
        "EFC processing logfile from {}",
        fmt_eac_datetime(summary.finish_ms)
    ));
    lines.push(String::new());

    // Album name: last path component of source_folder (or the whole string)
    let album_name = summary
        .source_folder
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or(&summary.source_folder);
    lines.push(album_name.to_string());
    lines.push(String::new());

    lines.push(eac_line("Source folder", &summary.source_folder));
    lines.push(eac_line("Run started", &fmt_eac_datetime(summary.start_ms)));
    lines.push(eac_line("Run finished", &fmt_eac_datetime(summary.finish_ms)));
    lines.push(eac_line("Worker threads", &summary.thread_count.to_string()));
    lines.push(eac_line("Retry limit", &summary.max_retries.to_string()));
    lines.push(eac_line(
        "Files discovered",
        &summary.counters.total_files.to_string(),
    ));
    lines.push(String::new());

    // ---- Per-file entries ----
    for event in events {
        lines.push("File".to_string());
        lines.push(String::new());
        lines.push(format!("     Filename {}", event.file));
        lines.push(String::new());
        lines.push(eac_line("Logged at", &event.time));
        lines.push(eac_line("Attempt", &event.attempt));
        lines.push(eac_line("Verification", &event.verification));

        match event.status {
            FileStatus::OK | FileStatus::WARN => {
                lines.push(eac_line("Original size", &fmt_bytes(event.before_size)));
                lines.push(eac_line("Compressed size", &fmt_bytes(event.after_size)));
                lines.push(eac_line(
                    "Net saved",
                    &format!(
                        "{} ({:.2}%)",
                        fmt_bytes(event.saved_bytes.unsigned_abs()),
                        event.compression_pct
                    ),
                ));
                lines.push(eac_line("Audio delta", "N/A"));
                lines.push(eac_line(
                    "Embedded MD5",
                    event.embedded_md5.as_deref().unwrap_or("N/A"),
                ));
                lines.push(eac_line(
                    "Calculated pre MD5",
                    event.source_hash.as_deref().unwrap_or("N/A"),
                ));
                lines.push(eac_line(
                    "Calculated post MD5",
                    event.output_hash.as_deref().unwrap_or("N/A"),
                ));
                if event.status == FileStatus::WARN {
                    // Write-back to the original URI failed; file saved to fc-output.
                    lines.push(eac_line("Write-back", &event.detail));
                    lines.push("     Copy OK (saved to fc-output — original unchanged)".to_string());
                } else {
                    // Metadata cleanup — only if detail is non-empty and not "none"
                    if !event.detail.is_empty() && event.detail != "none" {
                        lines.push(eac_line("Metadata cleanup", &event.detail));
                    }
                    lines.push("     Copy OK".to_string());
                }
            }
            FileStatus::RETRY => {
                lines.push("     Copy aborted".to_string());
                lines.push("     Retry scheduled".to_string());
                lines.push(eac_line("Reason", &event.detail));
            }
            FileStatus::FAIL => {
                lines.push("     Copy failed".to_string());
                lines.push(eac_line("Reason", &event.detail));
            }
        }

        lines.push(String::new());
    }

    // ---- Status report (matches New-EfcStatusReportLines) ----
    let c = &summary.counters;
    let pending = c.total_files.saturating_sub(c.processed);

    if c.successful > 0 {
        lines.push(format!(" {} file(s) processed successfully", c.successful));
    }
    if c.failed > 0 {
        lines.push(format!(" {} file(s) failed", c.failed));
    }
    if pending > 0 {
        lines.push(format!(" {} file(s) pending", pending));
    }
    if c.successful == 0 && c.failed == 0 && pending == 0 {
        lines.push(" 0 file(s) processed".to_string());
    }

    lines.push(String::new());
    if summary.run_canceled && pending > 0 {
        lines.push("Processing canceled by user".to_string());
    } else if c.failed > 0 {
        lines.push("Some files could not be verified".to_string());
    } else if c.warned > 0 && c.failed == 0 && pending == 0 {
        lines.push(format!(
            "All files compressed; {} file(s) saved to fc-output (write-back to original failed)",
            c.warned
        ));
    } else if c.successful > 0 && pending == 0 {
        lines.push("All files processed successfully".to_string());
    } else {
        lines.push("Processing complete".to_string());
    }

    lines.push(String::new());
    if c.failed > 0 || (summary.run_canceled && pending > 0) {
        lines.push("There were errors".to_string());
    } else if c.warned > 0 {
        lines.push("Write-back to original location failed — compressed files are in fc-output".to_string());
    } else {
        lines.push("No errors occurred".to_string());
    }
    lines.push(String::new());
    lines.push("End of status report".to_string());

    // ---- Top compression ----
    if !summary.top_compression.is_empty() {
        lines.push(String::new());
        lines.push("---- EFC Compression Notes".to_string());
        lines.push(String::new());
        for (i, tc) in summary.top_compression.iter().enumerate() {
            lines.push(format!(
                "  {}. Saved {} ({:.2}%) | {}",
                i + 1,
                fmt_bytes(tc.saved_bytes.unsigned_abs()),
                tc.saved_pct,
                tc.path
            ));
        }
    }

    // ---- Checksum footer (matches PS `==== Log checksum {sha256} ====`) ----
    let body = lines.join("\r\n");
    let checksum = sha256_hex(&body);
    format!("{}\r\n\r\n==== Log checksum {} ====\r\n", body, checksum)
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
            source_folder: "/music/album".to_string(),
            start_ms: 1_700_000_000_000,
            finish_ms: 1_700_000_030_000,
            thread_count: 4,
            max_retries: 3,
            run_canceled: false,
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
    fn test_efc_log_contains_exact_flac_cruncher_header() {
        let summary = make_run_summary(0, 0, 0, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(
            output.contains("Exact Flac Cruncher"),
            "output must contain 'Exact Flac Cruncher', got: {output}"
        );
    }

    #[test]
    fn test_efc_log_contains_processing_logfile_line() {
        let summary = make_run_summary(0, 0, 0, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(
            output.contains("EFC processing logfile from"),
            "output must contain 'EFC processing logfile from'"
        );
    }

    #[test]
    fn test_efc_log_contains_source_folder() {
        let summary = make_run_summary(0, 0, 0, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Source folder"), "must include Source folder label");
        assert!(output.contains("/music/album"), "must include source folder path");
    }

    #[test]
    fn test_efc_log_contains_worker_threads_and_retry_limit() {
        let summary = make_run_summary(1, 1, 1, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Worker threads"), "must include Worker threads");
        assert!(output.contains("Retry limit"), "must include Retry limit");
    }

    #[test]
    fn test_efc_log_contains_files_discovered() {
        let summary = make_run_summary(5, 5, 4, 1);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Files discovered"), "must include Files discovered label");
        assert!(output.contains("5"), "must include total file count");
    }

    #[test]
    fn test_efc_log_ok_event_shows_copy_ok() {
        let summary = make_run_summary(1, 1, 1, 0);
        let events = vec![make_file_event(FileStatus::OK, "/music/song.flac", "")];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("     Copy OK"), "OK event must show '     Copy OK'");
        assert!(output.contains("Filename /music/song.flac"), "must include Filename line");
        assert!(output.contains("Original size"), "must include Original size");
        assert!(output.contains("Compressed size"), "must include Compressed size");
        assert!(output.contains("Net saved"), "must include Net saved");
        assert!(output.contains("Embedded MD5"), "must include Embedded MD5");
        assert!(output.contains("Calculated pre MD5"), "must include Calculated pre MD5");
        assert!(output.contains("Calculated post MD5"), "must include Calculated post MD5");
    }

    #[test]
    fn test_efc_log_fail_event_shows_copy_failed_and_reason() {
        let summary = make_run_summary(1, 1, 0, 1);
        let events = vec![make_file_event(FileStatus::FAIL, "/music/fail.flac", "decode error")];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("     Copy failed"), "FAIL event must show '     Copy failed'");
        assert!(output.contains("Reason"), "must include Reason label");
        assert!(output.contains("decode error"), "must include the failure reason text");
    }

    #[test]
    fn test_efc_log_retry_event_shows_copy_aborted_and_retry_scheduled() {
        let summary = make_run_summary(1, 0, 0, 0);
        let events = vec![make_file_event(FileStatus::RETRY, "/music/retry.flac", "timeout")];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("     Copy aborted"), "RETRY event must show '     Copy aborted'");
        assert!(output.contains("     Retry scheduled"), "RETRY event must show '     Retry scheduled'");
        assert!(output.contains("Reason"), "must include Reason label");
    }

    #[test]
    fn test_efc_log_status_report_success() {
        let summary = make_run_summary(3, 3, 3, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("3 file(s) processed successfully"), "must include success count");
        assert!(output.contains("All files processed successfully"), "must include success summary");
        assert!(output.contains("No errors occurred"), "must include no-errors line");
        assert!(output.contains("End of status report"), "must include end line");
    }

    #[test]
    fn test_efc_log_status_report_failures() {
        let summary = make_run_summary(3, 3, 2, 1);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("2 file(s) processed successfully"));
        assert!(output.contains("1 file(s) failed"));
        assert!(output.contains("Some files could not be verified"));
        assert!(output.contains("There were errors"));
    }

    #[test]
    fn test_efc_log_status_report_canceled() {
        let mut summary = make_run_summary(5, 3, 3, 0);
        summary.run_canceled = true;
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Processing canceled by user"), "canceled must show cancellation message");
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
        assert!(output.contains("---- EFC Compression Notes"), "must include compression notes header");
        assert!(output.contains("/best.flac"), "must include best file path");
        assert!(output.contains("Saved"), "must include Saved label");
    }

    #[test]
    fn test_efc_log_checksum_footer_format() {
        let summary = make_run_summary(0, 0, 0, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(
            output.contains("==== Log checksum"),
            "must contain '==== Log checksum'"
        );
        assert!(output.contains("===="), "must end checksum line with ====");
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

    #[test]
    fn test_efc_log_metadata_cleanup_shown_when_detail_nonempty() {
        let summary = make_run_summary(1, 1, 1, 0);
        let mut event = make_file_event(FileStatus::OK, "/x.flac", "");
        event.detail = "MetadataCleanup 5.00 KB net (padding-only)".to_string();
        let output = generate_efc_log(&summary, &[event]);
        assert!(output.contains("Metadata cleanup"), "must show Metadata cleanup when detail is set");
    }

    #[test]
    fn test_efc_log_metadata_cleanup_hidden_when_detail_empty() {
        let summary = make_run_summary(1, 1, 1, 0);
        let event = make_file_event(FileStatus::OK, "/x.flac", "");
        let output = generate_efc_log(&summary, &[event]);
        assert!(!output.contains("Metadata cleanup"), "must not show Metadata cleanup when detail is empty");
    }

    #[test]
    fn test_efc_log_eac_value_line_label_padding() {
        let summary = make_run_summary(0, 0, 0, 0);
        let output = generate_efc_log(&summary, &[]);
        // Each field line starts with 5 spaces then the label
        assert!(output.contains("     Source folder"), "label must be indented with 5 spaces");
    }

    #[test]
    fn test_efc_log_album_name_is_last_folder_component() {
        let mut summary = make_run_summary(0, 0, 0, 0);
        summary.source_folder = "C:\\Music\\Beatles\\Abbey Road".to_string();
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Abbey Road"), "album name must be last path component");
    }
}
