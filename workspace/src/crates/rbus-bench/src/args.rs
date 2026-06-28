use std::path::PathBuf;

use rbus_core::warning::{Warning, WarningCode};

use crate::baseline::DEFAULT_BASELINE_PATH;
use crate::hot_path_syscall::HotPathOperation;
use crate::load_mode::{LoadMode, RateScope, parse_positive_rate_hz};
use crate::output_format::OutputFormat;
use crate::pace::messages_from_duration_and_rate;
use crate::topology::topology_by_name;
use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;
use crate::worker::{WorkerArguments, WorkerRole};

const DEFAULT_MESSAGES_PER_PUBLISHER: u32 = 100_000;
const DEFAULT_RUNS_PER_SCENARIO: u32 = 1;
const SUPPORTED_PACED_RATE_HZ: f64 = 30.0;
const RATE_HZ_COMPARISON_EPSILON: f64 = 0.000_001;

/// Benchmark metadata
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BenchArguments {
    /// Internal documentation
    pub mode: BenchMode,
}

/// Benchmark metadata
#[derive(Clone, Debug, PartialEq)]
pub struct SuiteOptions {
    /// Formats output
    pub output_format: OutputFormat,
    /// Publisher runtime metadata
    pub messages_per_publisher: u32,
    /// CLI metadata
    pub messages_explicit: bool,
    /// Benchmark metadata
    pub load_mode: LoadMode,
    /// Publisher runtime metadata
    pub rate_hz: Option<f64>,
    /// Publisher runtime metadata
    pub rate_scope: RateScope,
    /// Benchmark metadata
    pub duration_ms: Option<u64>,
    /// Benchmark metadata
    pub pace_spin_tail_us: u64,
    /// Benchmark metadata
    pub scenario_filter: Option<String>,
    /// Benchmark metadata
    pub wait_mode_filter: Option<WaitMode>,
    /// Benchmark metadata
    pub transport_filter: Option<TransportKind>,
    /// Payload metadata
    pub payload_size_filter: Option<usize>,
    /// Benchmark metadata
    pub runs_per_scenario: u32,
    /// Baseline CSV path
    pub baseline_path: PathBuf,
    /// Internal documentation
    pub gate_enabled: bool,
    /// Payload metadata
    pub matrix_smoke: bool,
}

/// Benchmark metadata
#[derive(Clone, Debug, Default, PartialEq)]
pub enum BenchMode {
    /// Internal documentation
    #[default]
    Missing,
    /// CLI metadata
    Help,
    /// Smoke mode
    Smoke(SuiteOptions),
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
            messages_explicit: false,
            load_mode: LoadMode::Saturated,
            rate_hz: None,
            rate_scope: RateScope::PerPublisher,
            duration_ms: None,
            pace_spin_tail_us: 0,
            scenario_filter: None,
            wait_mode_filter: None,
            transport_filter: None,
            payload_size_filter: None,
            runs_per_scenario: DEFAULT_RUNS_PER_SCENARIO,
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
        let mut requested_mode = RequestedMode::Missing;
        let mut suite_options = SuiteOptions::default();
        let mut saw_suite_option = false;
        let mut iterator = arguments.into_iter();
        let _program_name = iterator.next();

        while let Some(argument) = iterator.next() {
            match argument.as_str() {
                "--smoke" => requested_mode = RequestedMode::Smoke,
                "--suite" => requested_mode = RequestedMode::Suite,
                "--hot-path-syscall-probe" => {
                    let operation = iterator.next().ok_or_else(invalid_argument_warning)?;
                    let mode = BenchMode::HotPathSyscallProbe(
                        HotPathOperation::parse(&operation).ok_or_else(invalid_argument_warning)?,
                    );
                    return Ok(Self { mode });
                }
                "--suite-worker" => {
                    let role_argument = iterator.next().ok_or_else(invalid_argument_warning)?;
                    let role =
                        WorkerRole::parse(&role_argument).ok_or_else(invalid_argument_warning)?;
                    let mode = BenchMode::SuiteWorker(WorkerArguments::parse(role, iterator)?);
                    return Ok(Self { mode });
                }
                "--help" | "-h" => {
                    print_help();
                    return Ok(Self {
                        mode: BenchMode::Help,
                    });
                }
                _ => parse_suite_option(argument, &mut iterator, &mut suite_options)?,
            }
            saw_suite_option = true;
        }
        finalize_suite_options(&mut suite_options)?;
        let mode = match requested_mode {
            RequestedMode::Missing if saw_suite_option => {
                suite_options.gate_enabled = false;
                BenchMode::Suite(suite_options)
            }
            RequestedMode::Missing => BenchMode::Missing,
            RequestedMode::Smoke => BenchMode::Smoke(smoke_options(suite_options)),
            RequestedMode::Suite => BenchMode::Suite(suite_options),
        };
        Ok(Self { mode })
    }
}

fn print_help() {
    println!("rbus-bench --smoke [--load-mode saturated|paced] [--rate-hz N] [--duration-ms N]");
    println!(concat!(
        "rbus-bench --suite [--format json|csv] [--messages N] [--runs N] ",
        "[--baseline PATH] [--no-gate] [--load-mode saturated|paced] ",
        "[--rate-hz N] [--rate-scope per-publisher] [--duration-ms N] ",
        "[--pace-spin-tail-us N] [--scenario NAME] [--wait-mode NAME] ",
        "[--transport NAME] [--payload-size N]"
    ));
    println!("rbus-bench --hot-path-syscall-probe <publish|try-recv>");
}

fn parse_suite_option<I>(
    argument: String,
    iterator: &mut I,
    options: &mut SuiteOptions,
) -> Result<(), Warning>
where
    I: Iterator<Item = String>,
{
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
            options.messages_explicit = true;
            if options.messages_per_publisher == 0 {
                return Err(invalid_argument_warning());
            }
        }
        "--baseline" => {
            options.baseline_path =
                PathBuf::from(iterator.next().ok_or_else(invalid_argument_warning)?);
        }
        "--runs" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            options.runs_per_scenario = value
                .parse::<u32>()
                .map_err(|_| invalid_argument_warning())?;
            if options.runs_per_scenario == 0 {
                return Err(invalid_argument_warning());
            }
        }
        "--no-gate" => options.gate_enabled = false,
        "--matrix-smoke" => options.matrix_smoke = true,
        "--load-mode" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            options.load_mode = LoadMode::parse(&value).ok_or_else(invalid_argument_warning)?;
        }
        "--rate-hz" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            options.rate_hz = Some(parse_positive_rate_hz(&value)?);
        }
        "--rate-scope" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            options.rate_scope = RateScope::parse(&value).ok_or_else(invalid_argument_warning)?;
        }
        "--duration-ms" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            options.duration_ms = Some(
                value
                    .parse::<u64>()
                    .map_err(|_| invalid_argument_warning())?,
            );
            if options.duration_ms == Some(0) {
                return Err(invalid_argument_warning());
            }
        }
        "--pace-spin-tail-us" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            options.pace_spin_tail_us = value
                .parse::<u64>()
                .map_err(|_| invalid_argument_warning())?;
        }
        "--scenario" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            if topology_by_name(&value).is_none() {
                return Err(invalid_argument_warning());
            }
            options.scenario_filter = Some(value);
        }
        "--wait-mode" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            options.wait_mode_filter =
                Some(WaitMode::parse(&value).ok_or_else(invalid_argument_warning)?);
        }
        "--transport" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            options.transport_filter =
                Some(TransportKind::parse(&value).ok_or_else(invalid_argument_warning)?);
        }
        "--payload-size" => {
            let value = iterator.next().ok_or_else(invalid_argument_warning)?;
            let payload_size = value
                .parse::<usize>()
                .map_err(|_| invalid_argument_warning())?;
            if payload_size == 0 {
                return Err(invalid_argument_warning());
            }
            options.payload_size_filter = Some(payload_size);
        }
        _ => return Err(invalid_argument_warning()),
    }
    Ok(())
}

fn finalize_suite_options(options: &mut SuiteOptions) -> Result<(), Warning> {
    match options.load_mode {
        LoadMode::Saturated => {
            if options.rate_hz.is_some()
                || options.duration_ms.is_some()
                || options.pace_spin_tail_us != 0
            {
                return Err(invalid_argument_warning());
            }
        }
        LoadMode::Paced => {
            let rate_hz = options.rate_hz.ok_or_else(invalid_argument_warning)?;
            let duration_ms = options.duration_ms.ok_or_else(invalid_argument_warning)?;
            if (rate_hz - SUPPORTED_PACED_RATE_HZ).abs() > RATE_HZ_COMPARISON_EPSILON {
                return Err(invalid_argument_warning());
            }
            if options.messages_explicit {
                return Err(invalid_argument_warning());
            }
            options.messages_per_publisher = messages_from_duration_and_rate(duration_ms, rate_hz)?;
        }
    }
    Ok(())
}

fn smoke_options(mut options: SuiteOptions) -> SuiteOptions {
    options.matrix_smoke = true;
    options.gate_enabled = false;
    options
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestedMode {
    Missing,
    Smoke,
    Suite,
}

fn invalid_argument_warning() -> Warning {
    Warning::new(
        WarningCode::InvalidConfig,
        "rbus-bench",
        "BenchArguments::parse",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paced_mode_rejects_messages_and_duration_together() {
        let arguments = arguments(&[
            "rbus-bench",
            "--suite",
            "--load-mode",
            "paced",
            "--rate-hz",
            "30",
            "--duration-ms",
            "1000",
            "--messages",
            "30",
        ]);

        assert!(BenchArguments::parse(arguments).is_err());
    }

    #[test]
    fn paced_mode_sets_message_count_from_duration_and_rate() -> Result<(), Warning> {
        let arguments = arguments(&[
            "rbus-bench",
            "--suite",
            "--load-mode",
            "paced",
            "--rate-hz",
            "30",
            "--duration-ms",
            "1000",
        ]);

        let parsed = BenchArguments::parse(arguments)?;
        match parsed.mode {
            BenchMode::Suite(options) => {
                assert_eq!(options.messages_per_publisher, 30);
                Ok(())
            }
            _ => Err(invalid_argument_warning()),
        }
    }

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }
}
