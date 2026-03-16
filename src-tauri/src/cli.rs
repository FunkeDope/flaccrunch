//! CLI mode: process FLAC files without launching the GUI.

use crate::fs::scanner::{cleanup_stale_temps, scan_for_flac_files};
use crate::pipeline::job::{execute_job, ProcessingContext};
use crate::pipeline::queue::JobQueue;
use crate::pipeline::stages::PipelineEvent;
use crate::state::run_state::{FileEvent, FileStatus, RunCounters};
use crate::util::platform::{default_log_folder, default_thread_count};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Parsed CLI arguments.
pub struct CliArgs {
    pub paths: Vec<String>,
    pub silent: bool,
    pub threads: Option<usize>,
    pub retries: u32,
    pub log_dir: Option<String>,
    pub help: bool,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            silent: false,
            threads: None,
            retries: 3,
            log_dir: None,
            help: false,
        }
    }
}

/// Parse CLI args (not including argv[0]).
pub fn parse_cli_args(args: &[String]) -> CliArgs {
    let mut result = CliArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-silent" | "--silent" => result.silent = true,
            "-help" | "--help" | "-h" | "/?" => result.help = true,
            "-threads" | "--threads" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    if let Ok(n) = val.parse::<usize>() {
                        result.threads = Some(n.max(1));
                    }
                }
            }
            "-retries" | "--retries" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    if let Ok(n) = val.parse::<u32>() {
                        result.retries = n;
                    }
                }
            }
            "-logdir" | "--logdir" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    result.log_dir = Some(val.clone());
                }
            }
            arg => {
                result.paths.push(arg.to_string());
            }
        }
        i += 1;
    }
    result
}

pub fn print_help() {
    println!("FlacCrunch v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("  flaccrunch [paths...] [options]");
    println!();
    println!("OPTIONS:");
    println!("  -silent           Suppress GUI, print progress to console");
    println!("  -threads <N>      Worker thread count (default: CPU count - 1)");
    println!("  -retries <N>      Max retries per file (default: 3)");
    println!("  -logdir <path>    Log output directory");
    println!("  -help             Show this help");
    println!();
    println!("EXAMPLES:");
    println!("  flaccrunch \"D:\\Music\"                    Open GUI with folder pre-loaded");
    println!("  flaccrunch -silent \"D:\\Music\"            Process silently, console output only");
    println!("  flaccrunch -silent -threads 4 \"D:\\Music\" \"D:\\More\"  Use 4 threads");
}

fn fmt_bytes(bytes: i64) -> String {
    let abs = bytes.unsigned_abs();
    if abs >= 1_048_576 {
        format!("{:.1} MB", abs as f64 / 1_048_576.0)
    } else if abs >= 1_024 {
        format!("{:.1} KB", abs as f64 / 1_024.0)
    } else {
        format!("{} B", abs)
    }
}

fn fmt_elapsed(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Run the CLI processing pipeline. Returns OS exit code.
pub fn run_cli(args: CliArgs) -> i32 {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(run_cli_async(args))
}

async fn run_cli_async(args: CliArgs) -> i32 {
    let paths: Vec<PathBuf> = args.paths.iter().map(PathBuf::from).collect();

    if paths.is_empty() {
        eprintln!("Error: No paths specified. Use -help for usage.");
        return 1;
    }

    let dir_paths: Vec<PathBuf> = paths.iter().filter(|p| p.is_dir()).cloned().collect();
    cleanup_stale_temps(&dir_paths);

    let scan = scan_for_flac_files(&paths);
    if scan.files.is_empty() {
        eprintln!("No FLAC files found in the specified paths.");
        return 1;
    }

    let thread_count = args
        .threads
        .unwrap_or_else(default_thread_count)
        .min(scan.files.len());

    let log_dir = args
        .log_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_log_folder);
    let run_dir = log_dir.join(format!(
        "run_{}",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    ));
    let scratch_dir = run_dir.join("scratch");
    let _ = std::fs::create_dir_all(&scratch_dir);

    println!("FlacCrunch v{}", env!("CARGO_PKG_VERSION"));
    println!(
        "Processing {} files | {} workers",
        scan.files.len(),
        thread_count
    );
    let total_size = scan.total_size;
    println!("{}", "-".repeat(72));

    let context = Arc::new(ProcessingContext {
        max_retries: args.retries,
        scratch_dir,
    });

    let queue = Arc::new(JobQueue::new(scan.files));
    let total_files = queue.total();
    let (event_tx, mut event_rx) = mpsc::channel::<PipelineEvent>(512);
    let cancel_token = CancellationToken::new();

    let mut handles = Vec::new();
    for worker_id in 0..thread_count {
        let q = Arc::clone(&queue);
        let ctx = Arc::clone(&context);
        let tx = event_tx.clone();
        let ct = cancel_token.clone();
        handles.push(tokio::spawn(async move {
            worker_loop(worker_id, q, ctx, tx, ct).await;
        }));
    }
    drop(event_tx);

    let mut counters = RunCounters { total_files, ..Default::default() };
    let start = std::time::Instant::now();

    while let Some(event) = event_rx.recv().await {
        if let PipelineEvent::FileCompleted { event: fe, .. } = event {
            print_file_event(&fe, &mut counters);
        }
    }

    for h in handles {
        let _ = h.await;
    }

    // Final newline after progress display
    eprintln!();
    println!("{}", "-".repeat(72));

    let elapsed = fmt_elapsed(start.elapsed().as_secs());
    let total_pct = if counters.total_saved_bytes > 0 && total_size > 0 {
        counters.total_saved_bytes as f64 / total_size as f64 * 100.0
    } else {
        0.0
    };

    println!(
        "Done!  {} files | {} OK | {} FAIL | Saved: {} ({:.1}%) | Art: {} | Elapsed: {}",
        counters.total_files,
        counters.successful,
        counters.failed,
        fmt_bytes(counters.total_saved_bytes),
        total_pct,
        fmt_bytes(counters.total_artwork_saved),
        elapsed,
    );

    if counters.failed > 0 { 1 } else { 0 }
}

fn print_file_event(fe: &FileEvent, counters: &mut RunCounters) {
    let name = std::path::Path::new(&fe.file)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| fe.file.clone());

    match fe.status {
        FileStatus::OK | FileStatus::WARN => {
            counters.successful += 1;
            if fe.status == FileStatus::WARN {
                counters.warned += 1;
            }
            counters.total_saved_bytes += fe.saved_bytes;
            counters.total_artwork_saved += fe.artwork_saved_bytes;
            let savings = if fe.saved_bytes > 0 {
                format!(
                    "saved {:>9}  ({:.1}%)",
                    fmt_bytes(fe.saved_bytes),
                    fe.compression_pct
                )
            } else {
                "no savings             ".to_string()
            };
            let tag = if fe.status == FileStatus::WARN { "WARN" } else { " OK " };
            println!(
                "[{}] {}  {:<50} {}  {}",
                fe.time, tag, name, savings, fe.verification
            );
            if fe.status == FileStatus::WARN && !fe.detail.is_empty() {
                println!("       ^ {}", fe.detail);
            }
        }
        FileStatus::FAIL => {
            counters.failed += 1;
            let detail = if !fe.detail.is_empty() {
                &fe.detail
            } else {
                &fe.verification
            };
            println!("[{}] FAIL {:<50} {}", fe.time, name, detail);
        }
        FileStatus::RETRY => {
            println!("[{}] RETRY {}", fe.time, name);
        }
    }

    counters.processed += 1;

    // Inline progress on stderr every 10 files
    if counters.processed.is_multiple_of(10) || counters.processed == counters.total_files {
        let pct = if counters.total_files > 0 {
            counters.processed as f64 / counters.total_files as f64 * 100.0
        } else {
            0.0
        };
        eprint!(
            "\rProgress: {}/{} ({:.1}%) | Saved: {}          ",
            counters.processed,
            counters.total_files,
            pct,
            fmt_bytes(counters.total_saved_bytes)
        );
    }
}

async fn worker_loop(
    worker_id: usize,
    queue: Arc<JobQueue>,
    context: Arc<ProcessingContext>,
    event_tx: mpsc::Sender<PipelineEvent>,
    cancel_token: CancellationToken,
) {
    loop {
        if cancel_token.is_cancelled() {
            break;
        }
        let item = match queue.dequeue() {
            Some(item) => item,
            None => break,
        };
        let result =
            execute_job(&item, worker_id, &context, &event_tx, cancel_token.clone()).await;

        let fe = make_file_event(&result);
        if result.status == FileStatus::FAIL && item.attempt < context.max_retries {
            queue.requeue_for_retry(item, result.attempt + 1);
        }
        let _ = event_tx
            .send(PipelineEvent::FileCompleted {
                worker_id,
                event: Box::new(fe),
                counters: RunCounters::default(),
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(val: &str) -> String {
        val.to_string()
    }

    fn args(vals: &[&str]) -> Vec<String> {
        vals.iter().map(|v| v.to_string()).collect()
    }

    // --- parse_cli_args ---

    #[test]
    fn test_defaults_on_empty_args() {
        let result = parse_cli_args(&[]);
        assert!(result.paths.is_empty());
        assert!(!result.silent);
        assert!(result.threads.is_none());
        assert_eq!(result.retries, 3);
        assert!(result.log_dir.is_none());
        assert!(!result.help);
    }

    #[test]
    fn test_silent_short_flag() {
        let result = parse_cli_args(&args(&["-silent"]));
        assert!(result.silent);
    }

    #[test]
    fn test_silent_long_flag() {
        let result = parse_cli_args(&args(&["--silent"]));
        assert!(result.silent);
    }

    #[test]
    fn test_help_short_dash() {
        let result = parse_cli_args(&args(&["-help"]));
        assert!(result.help);
    }

    #[test]
    fn test_help_long_flag() {
        let result = parse_cli_args(&args(&["--help"]));
        assert!(result.help);
    }

    #[test]
    fn test_help_h_flag() {
        let result = parse_cli_args(&args(&["-h"]));
        assert!(result.help);
    }

    #[test]
    fn test_help_windows_style() {
        let result = parse_cli_args(&args(&["/?"]));
        assert!(result.help);
    }

    #[test]
    fn test_threads_valid() {
        let result = parse_cli_args(&args(&["-threads", "4"]));
        assert_eq!(result.threads, Some(4));
    }

    #[test]
    fn test_threads_zero_clamped_to_one() {
        let result = parse_cli_args(&args(&["-threads", "0"]));
        assert_eq!(result.threads, Some(1));
    }

    #[test]
    fn test_threads_no_value_does_not_crash() {
        let result = parse_cli_args(&args(&["-threads"]));
        assert!(result.threads.is_none());
    }

    #[test]
    fn test_threads_invalid_value_ignored() {
        let result = parse_cli_args(&args(&["-threads", "invalid"]));
        assert!(result.threads.is_none());
    }

    #[test]
    fn test_threads_long_flag() {
        let result = parse_cli_args(&args(&["--threads", "8"]));
        assert_eq!(result.threads, Some(8));
    }

    #[test]
    fn test_retries_set() {
        let result = parse_cli_args(&args(&["-retries", "5"]));
        assert_eq!(result.retries, 5);
    }

    #[test]
    fn test_retries_zero() {
        let result = parse_cli_args(&args(&["-retries", "0"]));
        assert_eq!(result.retries, 0);
    }

    #[test]
    fn test_logdir_set() {
        let result = parse_cli_args(&args(&["-logdir", "/some/path"]));
        assert_eq!(result.log_dir, Some(s("/some/path")));
    }

    #[test]
    fn test_logdir_long_flag() {
        let result = parse_cli_args(&args(&["--logdir", "/another/path"]));
        assert_eq!(result.log_dir, Some(s("/another/path")));
    }

    #[test]
    fn test_unknown_flags_treated_as_paths() {
        let result = parse_cli_args(&args(&["/music", "/more"]));
        assert_eq!(result.paths, vec![s("/music"), s("/more")]);
    }

    #[test]
    fn test_combined_flags_and_paths() {
        let result = parse_cli_args(&args(&["-silent", "-threads", "2", "/music", "/more"]));
        assert!(result.silent);
        assert_eq!(result.threads, Some(2));
        assert_eq!(result.paths, vec![s("/music"), s("/more")]);
        assert!(!result.help);
    }

    #[test]
    fn test_all_options_together() {
        let result = parse_cli_args(&args(&[
            "-silent",
            "--threads", "4",
            "-retries", "7",
            "-logdir", "/logs",
            "/path/to/music",
        ]));
        assert!(result.silent);
        assert_eq!(result.threads, Some(4));
        assert_eq!(result.retries, 7);
        assert_eq!(result.log_dir, Some(s("/logs")));
        assert_eq!(result.paths, vec![s("/path/to/music")]);
    }

    #[test]
    fn test_help_does_not_affect_other_fields() {
        let result = parse_cli_args(&args(&["--help"]));
        assert!(result.help);
        assert!(!result.silent);
        assert!(result.threads.is_none());
    }
}

fn make_file_event(result: &crate::pipeline::stages::JobResult) -> FileEvent {
    FileEvent {
        time: chrono::Local::now().format("%H:%M:%S").to_string(),
        status: result.status.clone(),
        file: result.file_path.clone(),
        attempt: format!("{}", result.attempt),
        verification: result.verification.clone(),
        before_size: result.before_size,
        after_size: result.after_size,
        saved_bytes: result.saved_bytes,
        compression_pct: result.compression_pct,
        detail: result.error.clone().unwrap_or_default(),
        source_hash: result.source_hash.clone(),
        output_hash: result.output_hash.clone(),
        embedded_md5: result.embedded_md5.clone(),
        artwork_saved_bytes: result.artwork_result.as_ref().map(|a| a.saved_bytes).unwrap_or(0),
        artwork_raw_saved_bytes: result
            .artwork_result
            .as_ref()
            .map(|a| a.raw_saved_bytes)
            .unwrap_or(0),
        artwork_blocks_optimized: result
            .artwork_result
            .as_ref()
            .map(|a| a.blocks_optimized)
            .unwrap_or(0),
    }
}
