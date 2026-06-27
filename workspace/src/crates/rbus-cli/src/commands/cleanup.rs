use std::process::ExitCode;

use crate::args::CliArguments;
use crate::control_client::{print_result, request_daemon_text};
use rbus_ipc::control::opcode::Opcode;

pub fn run(arguments: &CliArguments) -> ExitCode {
    print_result(request_daemon_text(arguments, Opcode::Cleanup, &[]))
}
