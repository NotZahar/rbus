use std::ffi::CString;

use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const INVALID_MQD: libc::mqd_t = -1 as libc::mqd_t;
const POSIX_MQ_CHUNK_BYTES: usize = 1_024;
const POSIX_MQ_MAX_MESSAGES: libc::c_long = 10;

pub(crate) struct PosixMessageQueue {
    descriptor: libc::mqd_t,
    name: CString,
    owner: bool,
    receive_buffer: Vec<u8>,
}

impl PosixMessageQueue {
    pub(crate) fn create(name: CString) -> Result<Self, Warning> {
        unlink_queue(&name)?;
        // SAFETY: zero is a valid initial value for `mq_attr`; public fields are set below.
        let mut attributes = unsafe { core::mem::zeroed::<libc::mq_attr>() };
        attributes.mq_flags = 0;
        attributes.mq_maxmsg = POSIX_MQ_MAX_MESSAGES;
        attributes.mq_msgsize = POSIX_MQ_CHUNK_BYTES as libc::c_long;
        attributes.mq_curmsgs = 0;
        // SAFETY: `name` is a valid nul-terminated C string and `attributes` lives for the call.
        let descriptor = unsafe {
            libc::mq_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_RDONLY | libc::O_CLOEXEC,
                libc::S_IRUSR | libc::S_IWUSR,
                core::ptr::addr_of!(attributes),
            )
        };
        Self::from_descriptor(descriptor, name, true)
    }

    pub(crate) fn open(name: CString) -> Result<Self, Warning> {
        // SAFETY: `name` is a valid nul-terminated C string and flags do not require varargs.
        let descriptor = unsafe { libc::mq_open(name.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC) };
        Self::from_descriptor(descriptor, name, false)
    }

    pub(crate) fn send(&mut self, payload: &[u8]) -> Result<(), Warning> {
        for chunk in payload.chunks(POSIX_MQ_CHUNK_BYTES) {
            // SAFETY: descriptor is owned by this wrapper and chunk points to initialized bytes.
            let result = unsafe {
                libc::mq_send(
                    self.descriptor,
                    chunk.as_ptr().cast::<libc::c_char>(),
                    chunk.len(),
                    0,
                )
            };
            if result != 0 {
                return Err(warning_from_errno("mq_send"));
            }
        }
        Ok(())
    }

    pub(crate) fn receive(&mut self, payload: &mut [u8]) -> Result<(), Warning> {
        let mut offset = 0_usize;
        while offset < payload.len() {
            // SAFETY: descriptor is owned by this wrapper and receive_buffer is writable.
            let received = unsafe {
                libc::mq_receive(
                    self.descriptor,
                    self.receive_buffer.as_mut_ptr().cast::<libc::c_char>(),
                    self.receive_buffer.len(),
                    core::ptr::null_mut(),
                )
            };
            if received < 0 {
                return Err(warning_from_errno("mq_receive"));
            }
            let received = usize::try_from(received).map_err(|_| invalid_argument())?;
            let end = offset.saturating_add(received);
            if end > payload.len() {
                return Err(invalid_argument());
            }
            payload[offset..end].copy_from_slice(&self.receive_buffer[..received]);
            offset = end;
        }
        Ok(())
    }

    fn from_descriptor(
        descriptor: libc::mqd_t,
        name: CString,
        owner: bool,
    ) -> Result<Self, Warning> {
        if descriptor == INVALID_MQD {
            return Err(warning_from_errno("mq_open"));
        }
        Ok(Self {
            descriptor,
            name,
            owner,
            receive_buffer: vec![0_u8; POSIX_MQ_CHUNK_BYTES],
        })
    }
}

impl Drop for PosixMessageQueue {
    fn drop(&mut self) {
        // SAFETY: descriptor came from successful `mq_open` and is closed once here.
        let _ = unsafe { libc::mq_close(self.descriptor) };
        if self.owner {
            let _ = unlink_queue(&self.name);
        }
    }
}

fn unlink_queue(name: &CString) -> Result<(), Warning> {
    // SAFETY: name is a valid nul-terminated C string.
    let result = unsafe { libc::mq_unlink(name.as_ptr()) };
    if result == 0 || current_errno() == libc::ENOENT {
        Ok(())
    } else {
        Err(warning_from_errno("mq_unlink"))
    }
}

fn current_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EINVAL)
}

fn warning_from_errno(operation: &'static str) -> Warning {
    Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
        .with_os_errno(current_errno())
}

fn invalid_argument() -> Warning {
    Warning::new(
        WarningCode::InvalidConfig,
        COMPONENT_RBUS_BENCH,
        "posix_message_queue",
    )
}
