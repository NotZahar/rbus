mod allocation;
mod args;
mod baseline;
mod blocking_wait;
mod daemon_process;
mod harness;
mod hot_path_syscall;
mod metrics;
mod output_format;
mod payload;
mod pinning;
mod process;
mod report;
mod scenarios;
mod suite;
mod suite_matrix;
mod suite_ready;
mod suite_repetition;
mod suite_worker_output;
mod syscall_measurement;
mod system_metrics;
mod timestamp_payload;
mod topology;
mod transport_kind;
mod transports;
mod wait_mode;
mod worker;

use std::process::ExitCode;

const WARNING_EXIT_CODE: u8 = 2;

#[global_allocator]
static GLOBAL_ALLOCATOR: allocation::CountingAllocator = allocation::CountingAllocator;

fn main() -> ExitCode {
    match args::BenchArguments::parse(std::env::args()) {
        Ok(arguments) => harness::run(arguments),
        Err(warning) => {
            eprintln!(
                "warning [{}] {}::{}: {}\nstacktrace:\n{}",
                warning.code().as_str(),
                warning.component(),
                warning.operation(),
                warning.message(),
                std::backtrace::Backtrace::force_capture()
            );
            ExitCode::from(WARNING_EXIT_CODE)
        }
    }
}
