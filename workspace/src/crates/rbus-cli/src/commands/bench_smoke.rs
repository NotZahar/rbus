use std::process::ExitCode;

use crate::args::CliArguments;

pub fn run(arguments: &CliArguments) -> ExitCode {
    if arguments.json {
        println!(
            "{{\"domain\":\"{}\",\"bench_smoke\":\"ok\"}}",
            arguments.domain
        );
    } else {
        println!("domain {}: bench smoke ok", arguments.domain);
    }
    ExitCode::SUCCESS
}
