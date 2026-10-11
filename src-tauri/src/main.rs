// GUI binary. `windows_subsystem` is deliberately NOT set so that the legacy
// `-silent` CLI mode can write to the console. In GUI mode we call
// FreeConsole() immediately to hide the briefly-created console window.
//
// For scripted use prefer the dedicated `flaccrunch-cli` binary, which has no
// GUI dependencies at all.

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let cli_args = flaccrunch_lib::cli::parse_cli_args(&raw);

    if cli_args.help {
        flaccrunch_lib::cli::print_help("flaccrunch");
        return;
    }
    if cli_args.version {
        flaccrunch_lib::cli::print_version();
        return;
    }

    if cli_args.silent {
        let exit_code = flaccrunch_lib::cli::run_cli(cli_args);
        std::process::exit(exit_code);
    }

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
