use rbus_core::warning::{Warning, WarningCode};

/// Parsed command-line arguments for the daemon
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonArguments {
    /// Daemon domain name
    pub domain: String,
    /// Run in the foreground instead of daemonizing
    pub foreground: bool,
    /// Run the short startup smoke path
    pub smoke: bool,
}

impl DaemonArguments {
    /// Parses daemon arguments from an iterator
    pub fn parse<I>(arguments: I) -> Result<Self, Warning>
    where
        I: IntoIterator<Item = String>,
    {
        let mut domain = String::from("default");
        let mut foreground = true;
        let mut smoke = false;
        let mut iterator = arguments.into_iter();
        let _program_name = iterator.next();

        while let Some(argument) = iterator.next() {
            match argument.as_str() {
                "--domain" => {
                    domain = iterator.next().ok_or_else(|| {
                        Warning::new(
                            WarningCode::InvalidConfig,
                            "rbusd",
                            "DaemonArguments::parse",
                        )
                    })?;
                }
                "--foreground" => foreground = true,
                "--smoke" => smoke = true,
                "--help" | "-h" => {
                    print_help();
                    return Ok(Self {
                        domain,
                        foreground,
                        smoke,
                    });
                }
                _ => {
                    return Err(Warning::new(
                        WarningCode::InvalidConfig,
                        "rbusd",
                        "DaemonArguments::parse",
                    ));
                }
            }
        }

        if domain.is_empty() {
            return Err(Warning::new(
                WarningCode::InvalidConfig,
                "rbusd",
                "DaemonArguments::parse::domain",
            ));
        }

        Ok(Self {
            domain,
            foreground,
            smoke,
        })
    }
}

fn print_help() {
    println!("rbusd --domain <domain> [--foreground] [--smoke]");
    println!("user-level daemon/control plane rbus");
}
