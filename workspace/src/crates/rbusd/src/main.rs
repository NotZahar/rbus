mod args;
mod cleanup;
mod client_session;
mod daemon;
mod doctor;
mod domain;
mod inspect;
mod protocol_handler;
mod publisher_entry;
mod registry;
mod registry_types;
mod signal;
mod smoke;
mod stats;
mod stream_entry;
mod subscriber_entry;
mod warnings;

use std::process::ExitCode;

fn main() -> ExitCode {
    match args::DaemonArguments::parse(std::env::args()) {
        Ok(daemon_arguments) => daemon::run_daemon(daemon_arguments),
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
