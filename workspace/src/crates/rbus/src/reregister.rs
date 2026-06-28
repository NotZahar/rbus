use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::client::ControlClient;
use rbus_ipc::fd::OwnedFileDescriptor;

use crate::control::{check_control_session, reregister_publisher};
use crate::stream::StreamInner;

const COMPONENT_RBUS: &str = "rbus";
const HEARTBEAT_SLEEP_TICKS: usize = 25;
const RETRY_SLEEP_TICKS: usize = 5;
const SLEEP_TICK: Duration = Duration::from_millis(10);

pub(crate) struct PublisherReregisterWorker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl PublisherReregisterWorker {
    pub(crate) fn start(
        socket_path: PathBuf,
        stream: Arc<StreamInner>,
        stream_file_descriptor: OwnedFileDescriptor,
        topic_wait_file_descriptor: OwnedFileDescriptor,
        control_client: ControlClient,
    ) -> Result<Self, Warning> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("rbus-publisher-reregister".to_owned())
            .spawn(move || {
                publisher_reregister_loop(
                    socket_path,
                    stream,
                    stream_file_descriptor,
                    topic_wait_file_descriptor,
                    control_client,
                    thread_stop,
                );
            })
            .map_err(|error| match error.raw_os_error() {
                Some(errno) => Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_RBUS,
                    "PublisherReregisterWorker::start",
                )
                .with_os_errno(errno),
                None => Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_RBUS,
                    "PublisherReregisterWorker::start",
                ),
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for PublisherReregisterWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _join_result = handle.join();
        }
    }
}

fn publisher_reregister_loop(
    socket_path: PathBuf,
    stream: Arc<StreamInner>,
    stream_file_descriptor: OwnedFileDescriptor,
    topic_wait_file_descriptor: OwnedFileDescriptor,
    mut control_client: ControlClient,
    stop: Arc<AtomicBool>,
) {
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        if check_control_session(&mut control_client).is_ok() {
            sleep_ticks(&stop, HEARTBEAT_SLEEP_TICKS);
            continue;
        }
        loop {
            if stop.load(Ordering::Acquire) {
                return;
            }
            match reregister_publisher(
                &socket_path,
                stream.metadata(),
                &stream_file_descriptor,
                &topic_wait_file_descriptor,
            ) {
                Ok(reregistered) => {
                    stream.set_subscriber_count(reregistered.subscriber_count);
                    control_client = reregistered.control_client;
                    break;
                }
                Err(_warning) => sleep_ticks(&stop, RETRY_SLEEP_TICKS),
            }
        }
    }
}

fn sleep_ticks(stop: &AtomicBool, ticks: usize) {
    for _tick in 0..ticks {
        if stop.load(Ordering::Acquire) {
            return;
        }
        thread::sleep(SLEEP_TICK);
    }
}
