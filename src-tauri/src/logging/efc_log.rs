use crate::state::run_state::{FileEvent, FileStatus, RunSummary};
use crate::util::format::{format_bytes, format_elapsed, sha256_hex};
use std::time::Duration;

/// Generate the EFC-format final summary log text.
pub fn generate_efc_log(summary: &RunSummary, events: &[FileEvent]) -> String {
    let mut lines = Vec::new();

    lines.push("═══════════════════════════════════════════════════════════".to_string());
    lines.push("  FlacCrunch — Processing Summary".to_string());
    lines.push("═══════════════════════════════════════════════════════════".to_string());
    lines.push(String::new());

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
