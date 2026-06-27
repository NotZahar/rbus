use crate::args::HelpCommand;

/// CLI metadata
pub fn print_help() {
    println!("rbus-cli <command> [--domain <domain>] [--json]");
    println!("commands: list, stats, warnings, inspect, cleanup, doctor, bench-smoke");
}

/// CLI metadata
pub fn print_command_help(command: HelpCommand) {
    match command {
        HelpCommand::List => print_command_usage(
            "rbus-cli list [--domain <domain>] [--json]",
            "Prints topics, publishers, subscribers and streams from rbusd",
        ),
        HelpCommand::Stats => print_command_usage(
            "rbus-cli stats [--domain <domain>] [--json]",
            "Prints aggregated stream counters from rbusd",
        ),
        HelpCommand::Warnings => print_command_usage(
            "rbus-cli warnings [--domain <domain>] [--json]",
            "Prints warning counters and bounded stream samples",
        ),
        HelpCommand::Inspect => print_command_usage(
            "rbus-cli inspect [--topic <topic>] [--domain <domain>] [--json]",
            "Prints registry and stream metadata without reading payload",
        ),
        HelpCommand::Cleanup => print_command_usage(
            "rbus-cli cleanup [--domain <domain>] [--json]",
            "Cleans stale socket paths, orphaned records and inactive reader bits",
        ),
        HelpCommand::Doctor => print_command_usage(
            "rbus-cli doctor [--domain <domain>] [--json]",
            "Checks socket, ABI, memfd, eventfd, futex and resource limits",
        ),
        HelpCommand::BenchSmoke => print_command_usage(
            "rbus-cli bench-smoke [--domain <domain>] [--json]",
            "Prints short smoke status for CLI",
        ),
    }
}

fn print_command_usage(usage: &str, description: &str) {
    println!("{usage}");
    println!("{description}");
}
