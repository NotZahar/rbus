//! Domain handle used to create publishers and subscribers
use std::path::PathBuf;

use rbus_core::topic::{stable_topic_hash, validate_topic_name};
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::topic_wait::TopicWaitSegment;

use crate::config::TopicConfig;
use crate::control::{connect_daemon, register_publisher, register_subscriber};
use crate::publisher::Publisher;
use crate::stream::StreamInner;
use crate::subscriber::Subscriber;

const COMPONENT_RBUS: &str = "rbus";

/// Connection handle for one rbus daemon domain
pub struct Bus {
    socket_path: PathBuf,
}

impl Bus {
    /// Connects to a running daemon for `domain`
    pub fn connect(domain: &str) -> Result<Self, Warning> {
        if domain.is_empty() {
            return Err(Warning::new(
                WarningCode::InvalidConfig,
                COMPONENT_RBUS,
                "Bus::connect",
            ));
        }
        Ok(Self {
            socket_path: connect_daemon(domain)?,
        })
    }

    /// Registers a publisher for `topic` and creates its shared-memory stream
    pub fn publisher(&self, topic: &str, config: TopicConfig) -> Result<Publisher, Warning> {
        validate_topic_name(topic)?;
        config.validate_for_publisher()?;
        let topic_hash = stable_topic_hash(topic);
        let registration = register_publisher(&self.socket_path, topic_hash, config)?;
        let topic_wait = std::sync::Arc::new(TopicWaitSegment::map(
            &registration.topic_wait_file_descriptor,
        )?);
        let stream = StreamInner::attach(
            registration.metadata,
            registration.stream_generation,
            &registration.file_descriptor,
            registration.subscriber_count,
        )?;
        Publisher::new(
            self.socket_path.clone(),
            stream,
            topic_wait,
            registration.file_descriptor,
            registration.topic_wait_file_descriptor,
            registration.control_client,
        )
    }

    /// Registers a subscriber for `topic` and attaches existing streams
    pub fn subscriber(&self, topic: &str) -> Result<Subscriber, Warning> {
        validate_topic_name(topic)?;
        let topic_hash = stable_topic_hash(topic);
        let registration = register_subscriber(&self.socket_path, topic_hash)?;
        let topic_wait = std::sync::Arc::new(TopicWaitSegment::map(
            &registration.topic_wait_file_descriptor,
        )?);
        let subscriber = Subscriber::new(
            self.socket_path.clone(),
            topic_hash,
            registration.subscriber_id,
            registration.subscribed_generation,
            topic_wait,
            registration.topic_wait_file_descriptor,
            registration.control_client,
        );
        subscriber.attach_existing_streams()?;
        Ok(subscriber)
    }
}
