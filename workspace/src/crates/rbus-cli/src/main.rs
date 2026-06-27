mod args;
mod commands;
mod control_client;
mod output;

use std::process::ExitCode;

fn main() -> ExitCode {
    match args::CliArguments::parse(std::env::args()) {
        Ok(arguments) => commands::run(arguments),
        Err(warning) => {
            eprintln!(
                "warning [{}]: {}\nstacktrace:\n{}",
                warning.code().as_str(),
                warning.message(),
                std::backtrace::Backtrace::force_capture()
            );
            ExitCode::from(2)
        }
    }
}
