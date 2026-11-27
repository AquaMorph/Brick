//! Development tasks for Brick (B1). Run with `cargo xtask <task>`.

mod edsdk;
mod licences;

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  edsdk [ZIP...]       Install the Canon EDSDK from its download archives into
                       edsdk/ (R10, B2). Without arguments, every EDSDK*.zip in
                       the workspace root is used. See docs/edsdk.md.
  licences [--check]   List dependency licences. With --check, fail if any
                       dependency can only be used under GPL, LGPL, or AGPL (R6).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    let result = match args.as_slice() {
        ["edsdk", archives @ ..] => edsdk::run(archives),
        ["licences"] => licences::run(false),
        ["licences", "--check"] => licences::run(true),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
