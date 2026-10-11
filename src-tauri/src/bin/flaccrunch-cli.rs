//! Headless FlacCrunch CLI.
//!
//! Build without the GUI feature so the binary carries no Tauri/WebKit/GTK
//! dependencies:
//!
//! ```text
//! cargo build --release --no-default-features --bin flaccrunch-cli
//! ```

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = flaccrunch_lib::cli::parse_cli_args(&raw);

    if args.help {
        flaccrunch_lib::cli::print_help("flaccrunch-cli");
        return;
    }
    if args.version {
        flaccrunch_lib::cli::print_version();
        return;
    }

    std::process::exit(flaccrunch_lib::cli::run_cli(args));
}
