use rbus_core::warning::{Warning, WarningCode};

/// Top-level CLI command selected by the user
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliCommand {
    /// list
    List,
    /// stats
    Stats,
    /// warnings
    Warnings,
    /// inspect
    Inspect { topic: Option<String> },
    /// cleanup
    Cleanup,
    /// doctor
    Doctor,
    /// bench-smoke
    BenchSmoke,
    /// help
    Help { command: Option<HelpCommand> },
}

/// Command-specific help topic
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpCommand {
    /// list
    List,
    /// stats
    Stats,
    /// warnings
    Warnings,
    /// inspect
    Inspect,
    /// cleanup
    Cleanup,
    /// doctor
    Doctor,
    /// bench-smoke
    BenchSmoke,
}

/// Parsed CLI arguments shared by all commands
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliArguments {
    /// Daemon domain name
    pub domain: String,
    /// JSON output
    pub json: bool,
    /// Command to execute
    pub command: CliCommand,
}

impl CliArguments {
    /// Parses CLI arguments from an iterator
    pub fn parse<I>(arguments: I) -> Result<Self, Warning>
    where
        I: IntoIterator<Item = String>,
    {
        let mut iterator = arguments.into_iter();
        let _program_name = iterator.next();
        let mut command = match iterator.next() {
            Some(value) if is_help_argument(&value) => CliCommand::Help { command: None },
            Some(value) => parse_command(value, &mut iterator)?,
            None => CliCommand::Help { command: None },
        };

        let mut domain = String::from("default");
        let mut json = false;
        while let Some(argument) = iterator.next() {
            match argument.as_str() {
                "--domain" => {
                    domain = iterator.next().ok_or_else(|| {
                        Warning::new(
                            WarningCode::InvalidConfig,
                            "rbus-cli",
                            "CliArguments::parse",
                        )
                    })?;
                }
                "--json" => json = true,
                value if is_help_argument(value) => {
                    command = help_for_command(command);
                }
                _ => {
                    return Err(Warning::new(
                        WarningCode::InvalidConfig,
                        "rbus-cli",
                        "CliArguments::parse",
                    ));
                }
            }
        }

        Ok(Self {
            domain,
            json,
            command,
        })
    }
}

fn parse_command<I>(command_name: String, iterator: &mut I) -> Result<CliCommand, Warning>
where
    I: Iterator<Item = String>,
{
    match command_name.as_str() {
        "list" => Ok(CliCommand::List),
        "stats" => Ok(CliCommand::Stats),
        "warnings" => Ok(CliCommand::Warnings),
        "inspect" => {
            let mut topic = None;
            if let Some(argument) = iterator.next() {
                if is_help_argument(&argument) {
                    return Ok(CliCommand::Help {
                        command: Some(HelpCommand::Inspect),
                    });
                }
                if argument == "--topic" {
                    topic = Some(iterator.next().ok_or_else(|| {
                        Warning::new(
                            WarningCode::InvalidConfig,
                            "rbus-cli",
                            "parse_command::inspect::topic",
                        )
                    })?);
                } else {
                    return Err(Warning::new(
                        WarningCode::InvalidConfig,
                        "rbus-cli",
                        "parse_command::inspect",
                    ));
                }
            }
            Ok(CliCommand::Inspect { topic })
        }
        "cleanup" => Ok(CliCommand::Cleanup),
        "doctor" => Ok(CliCommand::Doctor),
        "bench-smoke" => Ok(CliCommand::BenchSmoke),
        _ => Err(Warning::new(
            WarningCode::InvalidConfig,
            "rbus-cli",
            "parse_command",
        )),
    }
}

fn help_for_command(command: CliCommand) -> CliCommand {
    let help_command = match command {
        CliCommand::List => Some(HelpCommand::List),
        CliCommand::Stats => Some(HelpCommand::Stats),
        CliCommand::Warnings => Some(HelpCommand::Warnings),
        CliCommand::Inspect { .. } => Some(HelpCommand::Inspect),
        CliCommand::Cleanup => Some(HelpCommand::Cleanup),
        CliCommand::Doctor => Some(HelpCommand::Doctor),
        CliCommand::BenchSmoke => Some(HelpCommand::BenchSmoke),
        CliCommand::Help { command } => command,
    };
    CliCommand::Help {
        command: help_command,
    }
}

fn is_help_argument(argument: &str) -> bool {
    argument == "--help" || argument == "-h"
}
