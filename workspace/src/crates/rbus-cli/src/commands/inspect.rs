use std::process::ExitCode;

use crate::args::CliArguments;
use crate::control_client::{print_result, request_daemon_text};
use rbus_ipc::control::opcode::Opcode;

pub fn run(arguments: &CliArguments, topic: Option<&str>) -> ExitCode {
    print_result(request_daemon_text(
        arguments,
        Opcode::Inspect,
        topic.unwrap_or("").as_bytes(),
    ))
}
