use crate::state::run_state::{FileEvent, FileStatus, RunSummary};
use crate::util::format::sha256_hex;
use chrono::{Local, TimeZone};

/// Format a value line in EAC style:
///   `     {label:<20} {value}`
fn eac_line(label: &str, value: &str) -> String {
    format!("     {:<20} {}", label, value)
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

/// Signed byte delta: `-1.50 KB` for a shrink, `+0.00 B` for no change.
fn fmt_delta(bytes: i64) -> String {
    let sign = if bytes > 0 {
        "-"
    } else if bytes < 0 {
        "+"
    } else {
        ""
    };
    format!("{sign}{}", fmt_bytes(bytes.unsigned_abs()).trim_start())
}

/// Format a percentage for log output while normalizing away negative zero.
fn fmt_pct(pct: f64) -> String {
    let normalized = if pct.abs() < 0.005 { 0.0 } else { pct };
    format!("{:.2}%", normalized)
}

/// Parent folder of a path (either separator); the path itself if it has none.
fn parent_folder(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(idx) if idx > 0 => &path[..idx],
        _ => path,
    }
}

/// Folders that contain files which failed, deduplicated with a failure
/// count and sorted by name. One folder is usually one album, so this is the
/// list of albums that need a replacement rip.
pub fn failed_folders(events: &[FileEvent]) -> Vec<(String, usize)> {
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for e in events.iter().filter(|e| e.status == FileStatus::FAIL) {
        *counts.entry(parent_folder(&e.file)).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(folder, n)| (folder.to_string(), n))
        .collect()
}

/// Format a Unix-ms timestamp as EAC log datetime: `"15. March 2026, 14:30"`.
fn fmt_eac_datetime(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            let day = dt.format("%d").to_string();
            let day = day.trim_start_matches('0');
            format!("{}. {}", day, dt.format("%B %Y, %-H:%M"))
        }
        chrono::LocalResult::None => ms.to_string(),
    }
}

/// Generate the EFC-format final summary log text, matching the PowerShell
/// `New-EfcFinalLogText` output.
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
        .rfind(|s| !s.is_empty())
        .unwrap_or(&summary.source_folder);
    lines.push(album_name.to_string());
    lines.push(String::new());

    lines.push(eac_line("Source folder", &summary.source_folder));
    lines.push(eac_line("Run started", &fmt_eac_datetime(summary.start_ms)));
    lines.push(eac_line(
        "Run finished",
        &fmt_eac_datetime(summary.finish_ms),
    ));
    lines.push(eac_line(
        "Worker threads",
        &summary.thread_count.to_string(),
    ));
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
                let audio_delta =
                    event.saved_bytes - event.artwork_saved_bytes - event.padding_saved_bytes;
                lines.push(eac_line("Original size", &fmt_bytes(event.before_size)));
                lines.push(eac_line("Compressed size", &fmt_bytes(event.after_size)));
                lines.push(eac_line(
                    "Net saved",
                    &format!(
                        "{} ({})",
                        fmt_bytes(event.saved_bytes.unsigned_abs()),
                        fmt_pct(event.compression_pct)
                    ),
                ));
                lines.push(eac_line("Audio delta", &fmt_delta(audio_delta)));
                if event.artwork_saved_bytes != 0 || event.artwork_blocks_optimized > 0 {
                    lines.push(eac_line(
                        "Artwork delta",
                        &format!(
                            "{} ({} block(s))",
                            fmt_delta(event.artwork_saved_bytes),
                            event.artwork_blocks_optimized
                        ),
                    ));
                }
                if event.padding_saved_bytes != 0 {
                    lines.push(eac_line(
                        "Padding removed",
                        &fmt_delta(event.padding_saved_bytes),
                    ));
                }
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
                    lines.push(eac_line("Write-back", &event.detail));
                    lines.push("     Copy warning (original file unchanged)".to_string());
                } else {
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
            FileStatus::SKIPPED => {
                lines.push(eac_line("Original size", &fmt_bytes(event.before_size)));
                lines.push("     Skipped — already crunched".to_string());
                if !event.detail.is_empty() {
                    lines.push(eac_line("Reason", &event.detail));
                }
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
    if c.skipped > 0 {
        lines.push(format!(" {} file(s) skipped (already crunched)", c.skipped));
    }
    if pending > 0 {
        lines.push(format!(" {} file(s) pending", pending));
    }
    if c.successful == 0 && c.failed == 0 && c.skipped == 0 && pending == 0 {
        lines.push(" 0 file(s) processed".to_string());
    }

    // A failed file means the source FLAC itself is bad (it could not be
    // decoded or did not verify); list where they live so the albums can be
    // re-ripped.
    let bad_folders = failed_folders(events);
    if !bad_folders.is_empty() {
        lines.push(String::new());
        lines.push(format!(" Folders with bad files ({}):", bad_folders.len()));
        for (folder, n) in &bad_folders {
            lines.push(format!("   {n:>3} bad  {folder}"));
        }
    }

    lines.push(String::new());
    if summary.run_canceled && pending > 0 {
        lines.push("Processing canceled by user".to_string());
    } else if c.failed > 0 {
        lines.push("Some files could not be verified".to_string());
    } else if c.warned > 0 && c.failed == 0 && pending == 0 {
        lines.push(format!(
            "All files compressed; {} file(s) could not be written back",
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
        lines.push(
            "Some compressed files could not be written back to their original location"
                .to_string(),
        );
    } else {
        lines.push("No errors occurred".to_string());
    }
    lines.push(String::new());
    lines.push("End of status report".to_string());

    // ---- Totals ----
    if c.successful > 0 {
        lines.push(String::new());
        lines.push("---- EFC Savings".to_string());
        lines.push(String::new());
        let total_pct = if c.total_original_bytes > 0 {
            c.total_saved_bytes as f64 / c.total_original_bytes as f64 * 100.0
        } else {
            0.0
        };
        let audio = c.total_saved_bytes - c.total_artwork_saved - c.total_padding_saved;
        lines.push(eac_line(
            "Total saved",
            &format!(
                "{} ({})",
                fmt_bytes(c.total_saved_bytes.unsigned_abs()),
                fmt_pct(total_pct)
            ),
        ));
        lines.push(eac_line("Audio", &fmt_delta(audio)));
        lines.push(eac_line(
            "Artwork",
            &format!(
                "{} ({} file(s), {} block(s))",
                fmt_delta(c.total_artwork_saved),
                c.artwork_optimized_files,
                c.artwork_optimized_blocks
            ),
        ));
        lines.push(eac_line("Padding", &fmt_delta(c.total_padding_saved)));
    }

    // ---- Top compression ----
    if !summary.top_compression.is_empty() {
        lines.push(String::new());
        lines.push("---- EFC Compression Notes".to_string());
        lines.push(String::new());
        for (i, tc) in summary.top_compression.iter().enumerate() {
            lines.push(format!(
                "  {}. Saved {} ({}) | {}",
                i + 1,
                fmt_bytes(tc.saved_bytes.unsigned_abs()),
                fmt_pct(tc.saved_pct),
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

    fn make_run_summary(
        total: usize,
        processed: usize,
        successful: usize,
        failed: usize,
    ) -> RunSummary {
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
            padding_saved_bytes: 0,
        }
    }

    #[test]
    fn test_efc_log_header_lines() {
        let summary = make_run_summary(5, 5, 4, 1);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("Exact Flac Cruncher"));
        assert!(output.contains("EFC processing logfile from"));
        assert!(output.contains("     Source folder"));
        assert!(output.contains("/music/album"));
        assert!(output.contains("Worker threads"));
        assert!(output.contains("Retry limit"));
        assert!(output.contains("Files discovered"));
    }

    #[test]
    fn test_efc_log_warn_does_not_reference_fc_output() {
        let mut summary = make_run_summary(1, 1, 1, 0);
        summary.counters.warned = 1;
        let event = make_file_event(
            FileStatus::WARN,
            "/music/album/track.flac",
            "Original file unchanged (write-back failed: permission denied)",
        );
        let output = generate_efc_log(&summary, &[event]);
        assert!(!output.contains("fc-output"));
        assert!(output.contains("Original file unchanged"));
        assert!(output.contains("Copy warning (original file unchanged)"));
    }

    #[test]
    fn test_efc_log_ok_event_shows_copy_ok_and_deltas() {
        let summary = make_run_summary(1, 1, 1, 0);
        let mut event = make_file_event(FileStatus::OK, "/music/song.flac", "");
        event.artwork_saved_bytes = 30;
        event.artwork_blocks_optimized = 1;
        event.padding_saved_bytes = 20;
        let output = generate_efc_log(&summary, &[event]);
        assert!(output.contains("     Copy OK"));
        assert!(output.contains("Filename /music/song.flac"));
        assert!(output.contains("Original size"));
        assert!(output.contains("Compressed size"));
        assert!(output.contains("Net saved"));
        // 100 saved − 30 art − 20 padding = 50 B of audio
        assert!(output.contains("Audio delta          -50.00 B"), "{output}");
        assert!(
            output.contains("Artwork delta        -30.00 B (1 block(s))"),
            "{output}"
        );
        assert!(output.contains("Padding removed      -20.00 B"), "{output}");
        assert!(output.contains("Embedded MD5"));
        assert!(output.contains("Calculated pre MD5"));
        assert!(output.contains("Calculated post MD5"));
    }

    #[test]
    fn test_efc_log_ok_event_without_artwork_omits_artwork_lines() {
        let summary = make_run_summary(1, 1, 1, 0);
        let event = make_file_event(FileStatus::OK, "/music/song.flac", "");
        let output = generate_efc_log(&summary, &[event]);
        assert!(!output.contains("Artwork delta"));
        assert!(!output.contains("Padding removed"));
        assert!(
            output.contains("Audio delta          -100.00 B"),
            "{output}"
        );
    }

    #[test]
    fn test_efc_log_fail_event_shows_copy_failed_and_reason() {
        let summary = make_run_summary(1, 1, 0, 1);
        let events = vec![make_file_event(
            FileStatus::FAIL,
            "/music/fail.flac",
            "decode error",
        )];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("     Copy failed"));
        assert!(output.contains("Reason"));
        assert!(output.contains("decode error"));
    }

    #[test]
    fn test_efc_log_retry_event_shows_copy_aborted_and_retry_scheduled() {
        let summary = make_run_summary(1, 0, 0, 0);
        let events = vec![make_file_event(
            FileStatus::RETRY,
            "/music/retry.flac",
            "timeout",
        )];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("     Copy aborted"));
        assert!(output.contains("     Retry scheduled"));
        assert!(output.contains("timeout"));
    }

    #[test]
    fn test_efc_log_status_report_success() {
        let summary = make_run_summary(3, 3, 3, 0);
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("3 file(s) processed successfully"));
        assert!(output.contains("All files processed successfully"));
        assert!(output.contains("No errors occurred"));
        assert!(output.contains("End of status report"));
        assert!(output.contains("---- EFC Savings"));
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
        assert!(output.contains("2 file(s) pending"));
        assert!(output.contains("Processing canceled by user"));
        assert!(output.contains("There were errors"));
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
        assert!(output.contains("---- EFC Compression Notes"));
        assert!(output.contains("/best.flac"));
    }

    #[test]
    fn test_efc_log_checksum_footer_is_valid() {
        let summary = make_run_summary(0, 0, 0, 0);
        let output = generate_efc_log(&summary, &[]);
        let (body, footer) = output
            .split_once("\r\n\r\n==== Log checksum ")
            .expect("footer present");
        let checksum = footer.trim_end_matches("\r\n").trim_end_matches(" ====");
        assert_eq!(checksum, sha256_hex(body));
    }

    #[test]
    fn test_efc_log_is_deterministic() {
        let summary = make_run_summary(2, 2, 2, 0);
        let events = vec![
            make_file_event(FileStatus::OK, "/a.flac", ""),
            make_file_event(FileStatus::OK, "/b.flac", ""),
        ];
        assert_eq!(
            generate_efc_log(&summary, &events),
            generate_efc_log(&summary, &events)
        );
    }

    #[test]
    fn test_efc_log_metadata_cleanup_shown_only_when_detail_nonempty() {
        let summary = make_run_summary(1, 1, 1, 0);
        let mut event = make_file_event(FileStatus::OK, "/x.flac", "");
        assert!(!generate_efc_log(&summary, &[event.clone()]).contains("Metadata cleanup"));
        event.detail = "MetadataCleanup 5.00 KB net (padding-only)".to_string();
        assert!(generate_efc_log(&summary, &[event]).contains("Metadata cleanup"));
    }

    #[test]
    fn test_efc_log_album_name_is_last_folder_component() {
        let mut summary = make_run_summary(0, 0, 0, 0);
        summary.source_folder = "C:\\Music\\Beatles\\Abbey Road".to_string();
        let output = generate_efc_log(&summary, &[]);
        assert!(output.contains("\r\nAbbey Road\r\n"));
    }

    #[test]
    fn test_efc_log_skipped_event_renders_status_and_status_report_line() {
        let mut summary = make_run_summary(2, 2, 1, 0);
        summary.counters.skipped = 1;
        let mut skipped =
            make_file_event(FileStatus::SKIPPED, "/music/done.flac", "Already crunched");
        skipped.before_size = 4096;
        skipped.after_size = 4096;
        skipped.saved_bytes = 0;
        let events = vec![
            make_file_event(FileStatus::OK, "/music/new.flac", ""),
            skipped,
        ];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains("Skipped — already crunched"));
        assert!(output.contains("1 file(s) skipped (already crunched)"));
        assert!(output.contains("1 file(s) processed successfully"));
    }

    #[test]
    fn test_efc_log_normalizes_negative_zero_percentage() {
        let summary = make_run_summary(1, 1, 1, 0);
        let mut event = make_file_event(FileStatus::OK, "/music/song.flac", "");
        event.saved_bytes = 44;
        event.compression_pct = -0.0001;
        let output = generate_efc_log(&summary, &[event]);
        assert!(output.contains("(0.00%)"), "{output}");
        assert!(!output.contains("(-0.00%)"));
    }

    #[test]
    fn test_failed_folders_dedupes_and_counts() {
        let events = vec![
            make_file_event(FileStatus::FAIL, "/music/Album B/02.flac", "bad"),
            make_file_event(FileStatus::OK, "/music/Album A/01.flac", ""),
            make_file_event(FileStatus::FAIL, "C:\\Music\\Album A\\03.flac", "bad"),
            make_file_event(FileStatus::FAIL, "/music/Album B/05.flac", "bad"),
            make_file_event(FileStatus::RETRY, "/music/Album C/01.flac", "transient"),
        ];
        assert_eq!(
            failed_folders(&events),
            vec![
                ("/music/Album B".to_string(), 2),
                ("C:\\Music\\Album A".to_string(), 1),
            ]
        );
        assert!(failed_folders(&[]).is_empty());
        assert_eq!(parent_folder("track.flac"), "track.flac");
    }

    #[test]
    fn test_efc_log_lists_folders_with_bad_files() {
        let summary = make_run_summary(3, 3, 1, 2);
        let events = vec![
            make_file_event(FileStatus::OK, "/music/Good/01.flac", ""),
            make_file_event(FileStatus::FAIL, "/music/Bad Album/01.flac", "decode error"),
            make_file_event(FileStatus::FAIL, "/music/Bad Album/02.flac", "decode error"),
        ];
        let output = generate_efc_log(&summary, &events);
        assert!(output.contains(" Folders with bad files (1):"), "{output}");
        assert!(output.contains("     2 bad  /music/Bad Album"), "{output}");
        assert!(!output.contains("/music/Good\r\n"));

        let clean = generate_efc_log(&make_run_summary(1, 1, 1, 0), &events[..1]);
        assert!(!clean.contains("Folders with bad files"));
    }

    #[test]
    fn test_fmt_delta_sign_convention() {
        assert_eq!(fmt_delta(1536), "-01.50 KB");
        assert_eq!(fmt_delta(-512), "+512.00 B");
        assert_eq!(fmt_delta(0), "00.00 B");
    }
}
