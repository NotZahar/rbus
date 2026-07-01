//! Helpers for sending CLI requests to the daemon

use std::process::ExitCode;

use rbus_core::warning::Warning;
use rbus_ipc::control::client::request_control_frame;
use rbus_ipc::control::io::CONTROL_FLAG_JSON;
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::unix_socket::default_socket_path;

use crate::args::CliArguments;

const MAX_DAEMON_RESPONSE_PAYLOAD: usize = 64 * 1024;

pub fn request_daemon_text(
    arguments: &CliArguments,
    opcode: Opcode,
    payload: &[u8],
) -> Result<String, Warning> {
    let flags = if arguments.json { CONTROL_FLAG_JSON } else { 0 };
    let socket_path = default_socket_path(&arguments.domain);
    let response = request_control_frame(
        &socket_path,
        opcode,
        flags,
        payload,
        MAX_DAEMON_RESPONSE_PAYLOAD,
    )?;
    String::from_utf8(response.payload).map_err(|_| {
        Warning::new(
            rbus_core::warning::WarningCode::ProtocolMismatch,
            "rbus-cli",
            "request_daemon_text::utf8",
        )
    })
}

pub fn print_result(result: Result<String, Warning>) -> ExitCode {
    match result {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(warning) => {
            eprintln!(
                "warning [{}]: {}\nstacktrace:\n{}",
                warning.code().as_str(),
                warning.message(),
                std::backtrace::Backtrace::force_capture()
            );
            ExitCode::from(2)
        }
    }
}
