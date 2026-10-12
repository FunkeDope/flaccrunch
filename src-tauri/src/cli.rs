//! Headless CLI: process FLAC files without a GUI.
//!
//! Used by the standalone `flaccrunch-cli` binary and by the GUI binary's
//! legacy `-silent` mode. Everything here is plain core code.

use crate::flac::encoder::CompressionProfile;
use crate::fs::scanner::{cleanup_stale_temps, scan_for_flac_files};
use crate::logging::efc_log::generate_efc_log;
use crate::pipeline::job::ProcessingContext;
use crate::pipeline::queue::JobQueue;
use crate::pipeline::sink::EventSink;
use crate::pipeline::stages::PipelineEvent;
use crate::pipeline::worker_pool::run_worker_pool;
use crate::state::run_state::{FileStatus, ProcessingStatus, RunState, RunSummary};
use crate::util::format::{format_bytes, format_elapsed};
use crate::util::platform::default_thread_count;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Exit codes.
pub const EXIT_OK: i32 = 0;
pub const EXIT_FAILURES: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_INTERRUPTED: i32 = 130;

/// Parsed CLI arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct CliArgs {
    pub paths: Vec<String>,
    /// GUI binary only: run headless instead of opening the window.
    pub silent: bool,
    pub threads: Option<usize>,
    pub retries: u32,
    /// `--log`: a file to write, or a folder to drop a timestamped log into.
    pub log: Option<String>,
    /// `--tag`: tag re-encoded files as crunched (FLACCRUNCH_INFO).
    pub tag: bool,
    /// `--skip-tagged`: skip files that already carry the tag.
    pub skip_tagged: bool,
    /// `--max`: non-subset maximum compression preset.
    pub max_compression: bool,
    /// Suppress per-file output; only print the final summary.
    pub quiet: bool,
    /// Scan and report what would be processed, then exit.
    pub dry_run: bool,
    pub help: bool,
    pub version: bool,
    /// Problems found while parsing (unknown flags, missing values).
    pub errors: Vec<String>,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            silent: false,
            threads: None,
            retries: 3,
            log: None,
            tag: false,
            skip_tagged: false,
            max_compression: false,
            quiet: false,
            dry_run: false,
            help: false,
            version: false,
            errors: Vec::new(),
        }
    }
}

/// Options that existed before v1.5.0, with what to use instead.
fn removed_option_hint(flag: &str) -> Option<&'static str> {
    match flag {
        "mark" | "mark-crunched" => Some("use --tag"),
        "skip-crunched" | "skip" => Some("use --skip-tagged"),
        "logdir" | "log-dir" | "logfile" | "log-file" => Some("use --log <file or folder>"),
        "lax" => Some("use --max"),
        "j" => Some("use -t"),
        "lpc-order" | "l" | "blocksize" | "b" | "apodization" | "A" | "rice-order"
        | "encoder-threads" => Some("removed; --max is the only compression switch"),
        _ => None,
    }
}

/// Parse CLI args (not including argv[0]). Accepts both `-flag` and `--flag`
/// spellings for every option. `--` ends option parsing.
pub fn parse_cli_args(args: &[String]) -> CliArgs {
    let mut result = CliArgs::default();
    let mut only_paths = false;
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].as_str();
        if only_paths || !arg.starts_with('-') || arg == "-" {
            result.paths.push(arg.to_string());
            i += 1;
            continue;
        }
        let flag = arg.trim_start_matches('-');
        // Allow `--threads=4` as well as `--threads 4`.
        let (flag, inline_value) = match flag.split_once('=') {
            Some((f, v)) => (f, Some(v.to_string())),
            None => (flag, None),
        };
        let mut take_value = |i: &mut usize, name: &str| -> Option<String> {
            if let Some(v) = inline_value.clone() {
                return Some(v);
            }
            *i += 1;
            match args.get(*i) {
                Some(v) => Some(v.clone()),
                None => {
                    result
                        .errors
                        .push(format!("Option --{name} requires a value"));
                    None
                }
            }
        };

        match flag {
            "" => only_paths = true,
            "silent" => result.silent = true,
            "help" | "h" => result.help = true,
            "version" | "V" => result.version = true,
            "quiet" | "q" => result.quiet = true,
            "dry-run" | "n" => result.dry_run = true,
            "tag" => result.tag = true,
            "skip-tagged" => result.skip_tagged = true,
            "max" => result.max_compression = true,
            "threads" | "t" => {
                if let Some(v) = take_value(&mut i, "threads") {
                    match v.parse::<usize>() {
                        Ok(n) => result.threads = Some(n.max(1)),
                        Err(_) => result.errors.push(format!("Invalid thread count: {v}")),
                    }
                }
            }
            "retries" | "r" => {
                if let Some(v) = take_value(&mut i, "retries") {
                    match v.parse::<u32>() {
                        Ok(n) => result.retries = n,
                        Err(_) => result.errors.push(format!("Invalid retry count: {v}")),
                    }
                }
            }
            "log" => {
                if let Some(v) = take_value(&mut i, "log") {
                    result.log = Some(v);
                }
            }
            other => match removed_option_hint(other) {
                Some(hint) => result
                    .errors
                    .push(format!("Unknown option: {arg} ({hint})")),
                None => result.errors.push(format!("Unknown option: {arg}")),
            },
        }
        i += 1;
    }

    if arg_is_help_request(args) {
        result.help = true;
    }
    result
}

/// `/?` is the Windows convention; it is not a dash option.
fn arg_is_help_request(args: &[String]) -> bool {
    args.iter().any(|a| a == "/?")
}

pub fn print_version() {
    println!(
        "FlacCrunch v{} (libFLAC {})",
        env!("CARGO_PKG_VERSION"),
        crate::flac::marker::libflac_version()
    );
}

pub fn print_help(bin_name: &str) {
    println!("FlacCrunch v{}", env!("CARGO_PKG_VERSION"));
    println!("Lossless FLAC recompression with verified audio and album-art optimization.");
    println!("A file is replaced only after the decoded audio of the output matches the");
    println!("original.");
    println!();
    println!("USAGE:");
    println!("  {bin_name} [options] <path>...");
    println!();
    println!("  <path> may be a folder (scanned recursively) or a .flac file.");
    println!();
    println!("OPTIONS:");
    println!("  -t, --threads <N>   Files processed at once (default: CPU count - 1)");
    println!("  -r, --retries <N>   Retries per failed file (default: 3)");
    println!("      --max           Maximum compression (non-subset FLAC: smaller, much");
    println!("                      slower; some old hardware players can't play it)");
    println!("      --tag           Tag processed files as crunched");
    println!("      --skip-tagged   Skip files that are already tagged");
    println!("      --log <path>    Write a log file; if <path> is a folder, a timestamped");
    println!("                      log is created inside it");
    println!("  -n, --dry-run       List the files that would be processed, change nothing");
    println!("  -q, --quiet         Only print the final summary");
    println!("  -V, --version       Show version");
    println!("  -h, --help          Show this help");
    if bin_name == "flaccrunch" {
        println!("      --silent        Run headless instead of opening the GUI");
    }
    println!();
    println!("EXIT CODES:");
    println!("  0  all files processed successfully");
    println!("  1  one or more files failed verification or could not be processed");
    println!("  2  usage error or no FLAC files found");
    println!();
    println!("EXAMPLES:");
    println!("  {bin_name} ~/Music");
    println!("  {bin_name} --tag --skip-tagged --log ~/logs ~/Music /mnt/nas/More");
    println!("  {bin_name} --max -t 4 \"D:\\Music\\Album\"");
}

/// The encoder profile: the subset preset, or the non-subset one for `--max`.
pub fn compression_profile(args: &CliArgs) -> CompressionProfile {
    if args.max_compression {
        CompressionProfile::max()
    } else {
        CompressionProfile::subset()
    }
}

/// Where `--log` writes: the path itself, or a timestamped file inside it when
/// it is an existing folder or ends with a path separator.
fn resolve_log_path(log: &str) -> PathBuf {
    let path = PathBuf::from(log);
    if path.is_dir() || log.ends_with('/') || log.ends_with('\\') {
        path.join(format!(
            "flaccrunch_{}.log",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        ))
    } else {
        path
    }
}

/// Prints pipeline events to the terminal.
struct ConsoleSink {
    quiet: bool,
    total: Arc<AtomicUsize>,
    stderr_is_tty: bool,
    /// Serialises interleaved stdout/stderr writes from the dispatcher.
    lock: Mutex<()>,
}

impl ConsoleSink {
    fn print_progress(&self, processed: usize, saved: i64, pending: usize) {
        if !self.stderr_is_tty {
            return;
        }
        let total = self.total.load(Ordering::Relaxed);
        let pct = if total > 0 {
            processed as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let mut err = std::io::stderr().lock();
        let _ = write!(
            err,
            "\r{processed}/{total} ({pct:.1}%) | {pending} pending | saved {}        ",
            format_bytes(saved, false)
        );
        let _ = err.flush();
    }

    fn clear_progress(&self) {
        if self.stderr_is_tty {
            let mut err = std::io::stderr().lock();
            let _ = write!(err, "\r{:<72}\r", "");
            let _ = err.flush();
        }
    }
}

impl EventSink for ConsoleSink {
    fn emit(&self, event: &PipelineEvent) {
        let PipelineEvent::FileCompleted {
            event: fe,
            counters,
            ..
        } = event
        else {
            return;
        };
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());

        if !self.quiet {
            self.clear_progress();
            let name = Path::new(&fe.file)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| fe.file.clone());
            let line = match fe.status {
                FileStatus::OK | FileStatus::WARN => {
                    let tag = if fe.status == FileStatus::WARN {
                        "WARN"
                    } else {
                        " OK "
                    };
                    let savings = if fe.saved_bytes > 0 {
                        format!(
                            "saved {:>10} ({:>5.1}%)",
                            format_bytes(fe.saved_bytes, false),
                            fe.compression_pct
                        )
                    } else {
                        format!("{:<26}", "no savings")
                    };
                    let mut s = format!(
                        "[{}] {tag} {name:<48} {savings}  {}",
                        fe.time, fe.verification
                    );
                    if fe.status == FileStatus::WARN && !fe.detail.is_empty() {
                        s.push_str(&format!("\n           ^ {}", fe.detail));
                    }
                    s
                }
                FileStatus::FAIL => {
                    let detail = if fe.detail.is_empty() {
                        &fe.verification
                    } else {
                        &fe.detail
                    };
                    format!("[{}] FAIL {name:<48} {detail}", fe.time)
                }
                FileStatus::RETRY => {
                    format!(
                        "[{}] RETRY {name:<47} attempt {} failed: {}",
                        fe.time, fe.attempt, fe.detail
                    )
                }
                FileStatus::SKIPPED => format!("[{}] SKIP {name:<48} already crunched", fe.time),
            };
            println!("{line}");
        }

        let pending = self
            .total
            .load(Ordering::Relaxed)
            .saturating_sub(counters.processed);
        self.print_progress(counters.processed, counters.total_saved_bytes, pending);
    }

    fn run_finished(&self) {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.clear_progress();
    }
}

/// Run the CLI processing pipeline. Returns the OS exit code.
pub fn run_cli(args: CliArgs) -> i32 {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Error: failed to create async runtime: {e}");
            return EXIT_USAGE;
        }
    };
    rt.block_on(run_cli_async(args))
}

async fn run_cli_async(args: CliArgs) -> i32 {
    if !args.errors.is_empty() {
        for e in &args.errors {
            eprintln!("Error: {e}");
        }
        eprintln!("Use --help for usage.");
        return EXIT_USAGE;
    }
    if args.paths.is_empty() {
        eprintln!("Error: no paths specified. Use --help for usage.");
        return EXIT_USAGE;
    }

    let paths: Vec<PathBuf> = args.paths.iter().map(PathBuf::from).collect();
    for p in &paths {
        if !p.exists() {
            eprintln!("Error: path does not exist: {}", p.display());
            return EXIT_USAGE;
        }
    }

    let dir_paths: Vec<PathBuf> = paths.iter().filter(|p| p.is_dir()).cloned().collect();
    cleanup_stale_temps(&dir_paths);

    let scan = scan_for_flac_files(&paths);
    for err in &scan.permission_errors {
        eprintln!("Warning: {err}");
    }
    if scan.files.is_empty() {
        eprintln!("No FLAC files found in the specified paths.");
        return EXIT_USAGE;
    }

    let thread_count = args
        .threads
        .unwrap_or_else(default_thread_count)
        .min(scan.files.len())
        .max(1);
    let total_files = scan.files.len();
    let total_size = scan.total_size;

    if args.dry_run {
        for f in &scan.files {
            println!(
                "{:>12}  {}",
                format_bytes(f.size as i64, false),
                f.path.display()
            );
        }
        println!(
            "{total_files} file(s), {} total, would use {thread_count} worker(s)",
            format_bytes(total_size as i64, false)
        );
        return EXIT_OK;
    }

    print_version();
    println!(
        "Processing {total_files} file(s) ({}) with {thread_count} worker(s){}{}",
        format_bytes(total_size as i64, false),
        if args.tag { " | tagging" } else { "" },
        if args.skip_tagged {
            " | skipping tagged"
        } else {
            ""
        },
    );
    let profile = compression_profile(&args);
    if profile.is_non_subset() {
        println!("Maximum compression: non-subset FLAC");
    }
    println!("{}", "-".repeat(72));

    let context = Arc::new(ProcessingContext {
        max_retries: args.retries,
        mark_as_crunched: args.tag,
        skip_crunched: args.skip_tagged,
        profile,
        encoder_threads: crate::util::platform::encoder_threads_for(thread_count),
    });
    let queue = Arc::new(JobQueue::new(scan.files));
    // No live appends in CLI mode: close now so workers exit once drained.
    queue.close();

    let run_state = Arc::new(RunState::new("cli".to_string(), thread_count));
    {
        let mut c = run_state
            .counters
            .write()
            .unwrap_or_else(|e| e.into_inner());
        c.total_files = total_files;
        c.total_original_bytes = total_size;
    }
    run_state.set_status(ProcessingStatus::Processing);

    let sink = Arc::new(ConsoleSink {
        quiet: args.quiet,
        total: Arc::new(AtomicUsize::new(total_files)),
        stderr_is_tty: std::io::IsTerminal::is_terminal(&std::io::stderr()),
        lock: Mutex::new(()),
    });

    // Ctrl+C: cancel the run, let workers clean up their temp files, then
    // print the summary for what did complete.
    let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let run_state = Arc::clone(&run_state);
        let interrupted = Arc::clone(&interrupted);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\nInterrupted — cancelling (in-flight files are left unchanged)…");
                interrupted.store(true, Ordering::SeqCst);
                run_state.set_status(ProcessingStatus::Cancelling);
                run_state.cancel_token.cancel();
            }
        });
    }

    let start_ms = chrono::Local::now().timestamp_millis();
    run_worker_pool(thread_count, queue, context, Arc::clone(&run_state), sink).await;
    let finish_ms = chrono::Local::now().timestamp_millis();
    let elapsed = run_state.start_time.elapsed();
    let run_canceled = interrupted.load(Ordering::SeqCst);
    run_state.set_status(ProcessingStatus::Complete);

    let counters = run_state.counters_snapshot();
    let total_pct = if counters.total_original_bytes > 0 {
        counters.total_saved_bytes as f64 / counters.total_original_bytes as f64 * 100.0
    } else {
        0.0
    };

    println!("{}", "-".repeat(72));
    println!(
        "Done{}  {} files | {} OK | {} FAIL | {} skipped | Saved: {} ({:.1}%) | Art: {} | Padding: {} | Elapsed: {}",
        if run_canceled { " (cancelled)" } else { "!" },
        counters.total_files,
        counters.successful,
        counters.failed,
        counters.skipped,
        format_bytes(counters.total_saved_bytes, false),
        total_pct,
        format_bytes(counters.total_artwork_saved, false),
        format_bytes(counters.total_padding_saved, false),
        format_elapsed(elapsed),
    );

    // Bad source files are the most useful thing a run can tell you: say
    // where they are, one line per folder (usually one album).
    let events = run_state
        .all_events
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let bad_folders = crate::logging::efc_log::failed_folders(&events);
    if !bad_folders.is_empty() {
        println!();
        println!(
            "Folders with bad files ({}) — originals left unchanged:",
            bad_folders.len()
        );
        for (folder, n) in &bad_folders {
            println!("  {n:>3} bad  {folder}");
        }
        if !args.quiet {
            println!();
            for fe in events.iter().filter(|e| e.status == FileStatus::FAIL) {
                println!("  FAIL  {}  ({})", fe.file, fe.detail);
            }
        }
    }

    // Optional EFC log.
    let log_path = args.log.as_deref().map(resolve_log_path);
    if let Some(path) = log_path {
        let summary = RunSummary {
            counters: counters.clone(),
            elapsed_secs: elapsed.as_secs(),
            top_compression: run_state
                .top_compression
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            source_folder: args.paths.join(", "),
            start_ms,
            finish_ms,
            thread_count,
            max_retries: args.retries,
            run_canceled,
        };
        let text = generate_efc_log(&summary, &events);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, text.as_bytes()) {
            Ok(()) => println!("Log written to {}", path.display()),
            Err(e) => eprintln!("Warning: could not write log {}: {e}", path.display()),
        }
    }

    if run_canceled {
        EXIT_INTERRUPTED
    } else if counters.failed > 0 {
        EXIT_FAILURES
    } else {
        EXIT_OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(vals: &[&str]) -> Vec<String> {
        vals.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn test_defaults_on_empty_args() {
        let r = parse_cli_args(&[]);
        assert_eq!(r, CliArgs::default());
    }

    #[test]
    fn test_flags_single_and_double_dash() {
        assert!(parse_cli_args(&args(&["-silent"])).silent);
        assert!(parse_cli_args(&args(&["--silent"])).silent);
        assert!(parse_cli_args(&args(&["-help"])).help);
        assert!(parse_cli_args(&args(&["--help"])).help);
        assert!(parse_cli_args(&args(&["-h"])).help);
        assert!(parse_cli_args(&args(&["/?"])).help);
        assert!(parse_cli_args(&args(&["--version"])).version);
        assert!(parse_cli_args(&args(&["-q"])).quiet);
        assert!(parse_cli_args(&args(&["--dry-run"])).dry_run);
        assert!(parse_cli_args(&args(&["--tag"])).tag);
        assert!(parse_cli_args(&args(&["--skip-tagged"])).skip_tagged);
        assert!(parse_cli_args(&args(&["--max"])).max_compression);
        assert!(compression_profile(&parse_cli_args(&args(&["--max"]))).lax);
        assert!(!compression_profile(&CliArgs::default()).lax);
    }

    #[test]
    fn test_removed_options_point_to_their_replacement() {
        let r = parse_cli_args(&args(&[
            "--mark",
            "--skip-crunched",
            "--logdir",
            "/l",
            "-b",
            "8",
        ]));
        assert_eq!(r.errors.len(), 4, "{:?}", r.errors);
        assert!(r.errors[0].contains("use --tag"));
        assert!(r.errors[1].contains("use --skip-tagged"));
        assert!(r.errors[2].contains("use --log"));
        assert!(r.errors[3].contains("--max is the only compression switch"));
    }

    #[test]
    fn test_threads_parsing() {
        assert_eq!(parse_cli_args(&args(&["-threads", "4"])).threads, Some(4));
        assert_eq!(parse_cli_args(&args(&["--threads=8"])).threads, Some(8));
        assert_eq!(parse_cli_args(&args(&["-t", "2"])).threads, Some(2));
        assert_eq!(parse_cli_args(&args(&["-threads", "0"])).threads, Some(1));
        let r = parse_cli_args(&args(&["-threads"]));
        assert!(r.threads.is_none());
        assert!(!r.errors.is_empty());
        let r = parse_cli_args(&args(&["-threads", "invalid"]));
        assert!(r.threads.is_none());
        assert!(r.errors[0].contains("Invalid thread count"));
    }

    #[test]
    fn test_retries_and_logs() {
        let r = parse_cli_args(&args(&["-retries", "5", "--log", "x.log"]));
        assert_eq!(r.retries, 5);
        assert_eq!(r.log.as_deref(), Some("x.log"));
        assert_eq!(parse_cli_args(&args(&["-retries", "0"])).retries, 0);
    }

    #[test]
    fn test_log_path_file_or_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("run.log");
        assert_eq!(resolve_log_path(&file.to_string_lossy()), file);
        let inside = resolve_log_path(&dir.path().to_string_lossy());
        assert_eq!(inside.parent(), Some(dir.path()));
        assert!(inside
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("flaccrunch_"));
        assert!(resolve_log_path("not-yet/").starts_with("not-yet"));
    }

    #[test]
    fn test_unknown_option_is_an_error_not_a_path() {
        let r = parse_cli_args(&args(&["-thread", "4", "/music"]));
        assert_eq!(r.errors, vec!["Unknown option: -thread".to_string()]);
        assert_eq!(r.paths, vec!["4".to_string(), "/music".to_string()]);
    }

    #[test]
    fn test_double_dash_ends_options() {
        let r = parse_cli_args(&args(&["--", "-weird-folder", "/music"]));
        assert!(r.errors.is_empty());
        assert_eq!(
            r.paths,
            vec!["-weird-folder".to_string(), "/music".to_string()]
        );
    }

    #[test]
    fn test_combined_flags_and_paths() {
        let r = parse_cli_args(&args(&[
            "-silent",
            "--threads",
            "4",
            "-retries",
            "7",
            "--tag",
            "/path/to/music",
            "D:\\More",
        ]));
        assert!(r.silent);
        assert_eq!(r.threads, Some(4));
        assert_eq!(r.retries, 7);
        assert!(r.tag);
        assert_eq!(
            r.paths,
            vec!["/path/to/music".to_string(), "D:\\More".to_string()]
        );
        assert!(r.errors.is_empty());
    }

    #[test]
    fn test_run_cli_usage_errors() {
        assert_eq!(run_cli(CliArgs::default()), EXIT_USAGE);
        let bad = CliArgs {
            paths: vec!["/x".into()],
            errors: vec!["boom".into()],
            ..CliArgs::default()
        };
        assert_eq!(run_cli(bad), EXIT_USAGE);
        let missing = CliArgs {
            paths: vec!["/definitely/not/here".into()],
            ..CliArgs::default()
        };
        assert_eq!(run_cli(missing), EXIT_USAGE);
    }

    #[test]
    fn test_run_cli_no_flac_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.mp3"), b"x").unwrap();
        let a = CliArgs {
            paths: vec![dir.path().to_string_lossy().to_string()],
            ..CliArgs::default()
        };
        assert_eq!(run_cli(a), EXIT_USAGE);
    }

    #[test]
    fn test_run_cli_dry_run_lists_without_touching_files() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.flac");
        std::fs::write(&f, b"not really flac").unwrap();
        let a = CliArgs {
            paths: vec![dir.path().to_string_lossy().to_string()],
            dry_run: true,
            ..CliArgs::default()
        };
        assert_eq!(run_cli(a), EXIT_OK);
        assert_eq!(std::fs::read(&f).unwrap(), b"not really flac");
    }

    #[test]
    fn test_run_cli_invalid_flac_fails_with_exit_1_and_writes_log() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("broken.flac");
        std::fs::write(&f, b"not really flac").unwrap();
        let log = dir.path().join("out.log");
        let a = CliArgs {
            paths: vec![dir.path().to_string_lossy().to_string()],
            retries: 1,
            quiet: true,
            log: Some(log.to_string_lossy().to_string()),
            ..CliArgs::default()
        };
        assert_eq!(run_cli(a), EXIT_FAILURES);
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("Exact Flac Cruncher"));
        assert!(text.contains("1 file(s) failed"));
        assert!(text.contains("broken.flac"));
        // Original untouched, no temp left behind.
        assert_eq!(std::fs::read(&f).unwrap(), b"not really flac");
        assert!(!dir.path().join("broken.tmp").exists());
    }

    #[test]
    fn test_run_cli_processes_file_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let copy = crate::test_util::small_flac(dir.path(), "track.flac", 2);
        let before = std::fs::metadata(&copy).unwrap().len();
        let logdir = dir.path().join("logs");

        let a = CliArgs {
            paths: vec![copy.to_string_lossy().to_string()],
            threads: Some(1),
            quiet: true,
            tag: true,
            log: Some(format!("{}/", logdir.to_string_lossy())),
            ..CliArgs::default()
        };
        assert_eq!(run_cli(a), EXIT_OK);
        let after = std::fs::metadata(&copy).unwrap().len();
        assert!(after <= before);
        assert!(crate::flac::marker::read_crunched_marker(&copy).unwrap());
        let logs: Vec<_> = std::fs::read_dir(&logdir).unwrap().flatten().collect();
        assert_eq!(logs.len(), 1);
        assert!(std::fs::read_to_string(logs[0].path())
            .unwrap()
            .contains("All files processed successfully"));

        // Second pass with --skip-tagged is a no-op.
        let a = CliArgs {
            paths: vec![copy.to_string_lossy().to_string()],
            threads: Some(1),
            quiet: true,
            skip_tagged: true,
            ..CliArgs::default()
        };
        assert_eq!(run_cli(a), EXIT_OK);
        assert_eq!(std::fs::metadata(&copy).unwrap().len(), after);
    }
}
