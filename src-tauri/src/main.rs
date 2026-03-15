// Note: windows_subsystem is NOT set here so that -silent CLI mode can write
// to the console. In GUI mode we call FreeConsole() immediately to hide the
// briefly-created console window before any UI appears.

fn main() {
    let raw: Vec<String> = std::env::args().collect();
    let cli_args = flaccrunch_lib::cli::parse_cli_args(&raw[1..]);

    if cli_args.help {
        flaccrunch_lib::cli::print_help();
        return;
    }

    if cli_args.silent {
        // CLI mode — run the pipeline, no GUI.
        let exit_code = flaccrunch_lib::cli::run_cli(cli_args);
        std::process::exit(exit_code);
    } else {
        // GUI mode — free the console on Windows to avoid a visible console window.
        #[cfg(windows)]
        {
            extern "system" {
                fn FreeConsole() -> i32;
            }
            unsafe {
                FreeConsole();
            }
        }
        flaccrunch_lib::run_with_startup_paths(cli_args.paths);
    }
}
