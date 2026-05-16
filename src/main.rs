#![forbid(unsafe_code)]

use std::process::ExitCode;

use owo_colors::OwoColorize;

fn main() -> ExitCode {
    match devctl::run_from(std::env::args_os()) {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            eprintln!("{} {error:#}", "error:".red().bold());
            ExitCode::from(devctl::EXIT_ERROR as u8)
        }
    }
}
