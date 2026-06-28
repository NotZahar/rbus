use std::path::PathBuf;

use rbus_core::warning::{Warning, WarningCode};

use crate::baseline::DEFAULT_BASELINE_PATH;
use crate::hot_path_syscall::HotPathOperation;
use crate::output_format::OutputFormat;
use crate::worker::{WorkerArguments, WorkerRole};

const DEFAULT_MESSAGES_PER_PUBLISHER: u32 = 4;

/// Benchmark metadata
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BenchArguments {
    /// Internal documentation
    pub mode: BenchMode,
}

/// Benchmark metadata
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuiteOptions {
    /// Formats output
    pub output_format: OutputFormat,
    /// Publisher runtime metadata
    pub messages_per_publisher: u32,
    /// Baseline CSV path
    pub baseline_path: PathBuf,
    /// Internal documentation
    pub gate_enabled: bool,
    /// Payload metadata
    pub matrix_smoke: bool,
}

/// Benchmark metadata
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BenchMode {
    /// Internal documentation
    #[default]
    Missing,
    /// CLI metadata
    Help,
    /// Smoke mode
    Smoke,
    /// Benchmark metadata
    Suite(SuiteOptions),
    /// Internal documentation
    HotPathSyscallProbe(HotPathOperation),
    /// Benchmark metadata
    SuiteWorker(WorkerArguments),
}

impl Default for SuiteOptions {
    fn default() -> Self {
        Self {
            output_format: OutputFormat::Json,
            messages_per_publisher: DEFAULT_MESSAGES_PER_PUBLISHER,
            baseline_path: PathBuf::from(DEFAULT_BASELINE_PATH),
            gate_enabled: true,
            matrix_smoke: false,
        }
    }
}

impl BenchArguments {
    /// Benchmark metadata
    pub fn parse<I>(arguments: I) -> Result<Self, Warning>
    where
        I: IntoIterator<Item = String>,
    {
        let mut mode = BenchMode::Missing;
        let mut iterator = arguments.into_iter();
        let _program_name = iterator.next();

        while let Some(argument) = iterator.next() {
            match argument.as_str() {
                "--smoke" => mode = BenchMode::Smoke,
                "--suite" => {
                    return Ok(Self {
                        mode: BenchMode::Suite(parse_suite_options(iterator)?),
                    });
                }
                "--hot-path-syscall-probe" => {
                    let operation = iterator.next().ok_or_else(invalid_argument_warning)?;
                    mode = BenchMode::HotPathSyscallProbe(
                        HotPathOperation::parse(&operation).ok_or_else(invalid_argument_warning)?,
                    );
                }
                "--suite-worker" => {
                    let role_argument = iterator.next().ok_or_else(invalid_argument_warning)?;
                    let role =
                        WorkerRole::parse(&role_argument).ok_or_else(invalid_argument_warning)?;
                    mode = BenchMode::SuiteWorker(WorkerArguments::parse(role, iterator)?);
                    return Ok(Self { mode });
                }
                "--help" | "-h" => {
                    print_help();
                    return Ok(Self {
                        mode: BenchMode::Help,
                    });
                }
                _ => {
                    return Err(invalid_argument_warning());
                }
            }
        }
        Ok(Self { mode })
    }
}

fn print_help() {
    println!("rbus-bench --smoke");
    println!("rbus-bench --suite [--format json|csv] [--messages N] [--baseline PATH] [--no-gate]");
    println!("rbus-bench --hot-path-syscall-probe <publish|try-recv>");
}

fn parse_suite_options<I>(mut iterator: I) -> Result<SuiteOptions, Warning>
where
    I: Iterator<Item = String>,
{
    let mut options = SuiteOptions::default();
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--format" => {
                let value = iterator.next().ok_or_else(invalid_argument_warning)?;
                options.output_format =
                    OutputFormat::parse(&value).ok_or_else(invalid_argument_warning)?;
            }
            "--messages" => {
                let value = iterator.next().ok_or_else(invalid_argument_warning)?;
                options.messages_per_publisher = value
                    .parse::<u32>()
                    .map_err(|_| invalid_argument_warning())?;
                if options.messages_per_publisher == 0 {
                    return Err(invalid_argument_warning());
                }
            }
            "--baseline" => {
                options.baseline_path =
                    PathBuf::from(iterator.next().ok_or_else(invalid_argument_warning)?);
            }
            "--no-gate" => options.gate_enabled = false,
            "--matrix-smoke" => options.matrix_smoke = true,
            _ => return Err(invalid_argument_warning()),
        }
    }
    Ok(options)
}

fn invalid_argument_warning() -> Warning {
    Warning::new(
        WarningCode::InvalidConfig,
        "rbus-bench",
        "BenchArguments::parse",
    )
}
