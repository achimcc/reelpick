//! The program around the library: read the configuration, then either pick
//! today's film or serve the pages. Every step fails closed.

use std::process::ExitCode;

const USAGE: &str = "usage: reelpick <pick|serve|--version>";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("--version") => {
            println!("reelpick {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
