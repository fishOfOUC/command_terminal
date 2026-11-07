#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        std::process::exit(2);
    }
    let result = if args[1] == "--plan" {
        runbook_update::run_plan(std::path::Path::new(&args[2]))
    } else if args[1] == "--recover" {
        runbook_update::run_recovery(std::path::Path::new(&args[2]))
    } else {
        std::process::exit(2);
    };
    if let Err(error) = result {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
