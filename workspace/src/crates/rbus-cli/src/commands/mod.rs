pub mod bench_smoke;
pub mod cleanup;
pub mod doctor;
pub mod inspect;
pub mod list;
pub mod stats;
pub mod warnings;

use std::process::ExitCode;

use crate::args::{CliArguments, CliCommand};
use crate::output::{print_command_help, print_help};

/// CLI metadata
pub fn run(arguments: CliArguments) -> ExitCode {
    match arguments.command {
        CliCommand::List => list::run(&arguments),
        CliCommand::Stats => stats::run(&arguments),
        CliCommand::Warnings => warnings::run(&arguments),
        CliCommand::Inspect { ref topic } => inspect::run(&arguments, topic.as_deref()),
        CliCommand::Cleanup => cleanup::run(&arguments),
        CliCommand::Doctor => doctor::run(&arguments),
        CliCommand::BenchSmoke => bench_smoke::run(&arguments),
        CliCommand::Help { command } => {
            if let Some(help_command) = command {
                print_command_help(help_command);
            } else {
                print_help();
            }
            ExitCode::SUCCESS
        }
    }
}
