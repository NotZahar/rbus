use std::backtrace::Backtrace;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::num::{ParseIntError, TryFromIntError};
use std::process::ExitCode;
use std::str::Utf8Error;
use std::time::Duration;

use rbus::Warning;

const EXAMPLE_FAILURE_EXIT_CODE: u8 = 2;

pub type ExampleResult<T> = Result<T, ExampleError>;

#[derive(Debug)]
pub enum ExampleError {
    Io(std::io::Error),
    Message(String),
    ParseInt(ParseIntError),
    TryFromInt(TryFromIntError),
    Utf8(Utf8Error),
    Warning(Warning),
}

impl ExampleError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl Display for ExampleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "io error: {error}"),
            Self::Message(message) => formatter.write_str(message),
            Self::ParseInt(error) => write!(formatter, "integer parse error: {error}"),
            Self::TryFromInt(error) => write!(formatter, "integer conversion error: {error}"),
            Self::Utf8(error) => write!(formatter, "utf8 error: {error}"),
            Self::Warning(warning) => write!(
                formatter,
                "warning [{}] component={} operation={}: {}",
                warning.code().as_str(),
                warning.component(),
                warning.operation(),
                warning.message()
            ),
        }
    }
}

impl Error for ExampleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::ParseInt(error) => Some(error),
            Self::TryFromInt(error) => Some(error),
            Self::Utf8(error) => Some(error),
            Self::Message(_) | Self::Warning(_) => None,
        }
    }
}

impl From<std::io::Error> for ExampleError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ParseIntError> for ExampleError {
    fn from(error: ParseIntError) -> Self {
        Self::ParseInt(error)
    }
}

impl From<TryFromIntError> for ExampleError {
    fn from(error: TryFromIntError) -> Self {
        Self::TryFromInt(error)
    }
}

impl From<Utf8Error> for ExampleError {
    fn from(error: Utf8Error) -> Self {
        Self::Utf8(error)
    }
}

impl From<Warning> for ExampleError {
    fn from(warning: Warning) -> Self {
        Self::Warning(warning)
    }
}

pub struct ParsedArguments {
    values: Vec<(&'static str, String)>,
}

impl ParsedArguments {
    pub fn required_string(&self, option_name: &'static str) -> ExampleResult<String> {
        self.values
            .iter()
            .find_map(|(existing_name, value)| {
                (*existing_name == option_name).then(|| value.clone())
            })
            .ok_or_else(|| ExampleError::message(format!("missing required option {option_name}")))
    }

    pub fn required_usize(&self, option_name: &'static str) -> ExampleResult<usize> {
        Ok(self.required_string(option_name)?.parse::<usize>()?)
    }

    pub fn required_duration_millis(&self, option_name: &'static str) -> ExampleResult<Duration> {
        Ok(Duration::from_millis(
            self.required_string(option_name)?.parse::<u64>()?,
        ))
    }
}

pub fn parse_arguments(
    usage: &'static str,
    allowed_options: &[&'static str],
) -> ExampleResult<ParsedArguments> {
    let mut values = Vec::with_capacity(allowed_options.len());
    let mut arguments = std::env::args().skip(1);
    while let Some(option_name) = arguments.next() {
        if option_name == "--help" || option_name == "-h" {
            println!("{usage}");
            std::process::exit(0);
        }
        let matched_option = allowed_options
            .iter()
            .copied()
            .find(|allowed_option| *allowed_option == option_name)
            .ok_or_else(|| {
                ExampleError::message(format!("unknown option {option_name}\nusage: {usage}"))
            })?;
        if values
            .iter()
            .any(|(existing_name, _)| *existing_name == matched_option)
        {
            return Err(ExampleError::message(format!(
                "duplicate option {option_name}\nusage: {usage}"
            )));
        }
        let value = arguments.next().ok_or_else(|| {
            ExampleError::message(format!("missing value for {option_name}\nusage: {usage}"))
        })?;
        values.push((matched_option, value));
    }
    Ok(ParsedArguments { values })
}

pub fn print_line(line: &str) -> ExampleResult<()> {
    println!("{line}");
    std::io::stdout().flush()?;
    Ok(())
}

pub fn run_example(run: impl FnOnce() -> ExampleResult<()>) -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "error: {error}\nstacktrace:\n{}",
                Backtrace::force_capture()
            );
            ExitCode::from(EXAMPLE_FAILURE_EXIT_CODE)
        }
    }
}
