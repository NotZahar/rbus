use rbus_core::ids::TopicHash;
use rbus_core::topic::{stable_topic_hash, validate_topic_name};
use rbus_core::warning::Warning;
use rbus_ipc::stream_segment::snapshot_stream_segment;

use crate::registry::DaemonRegistry;

/// Payload metadata
pub fn inspect_domain(
    registry: &DaemonRegistry,
    domain: &str,
    topic: Option<&str>,
    json: bool,
) -> Result<String, Warning> {
    let topic_hash = topic_filter_hash(topic)?;
    if json {
        inspect_json(registry, domain, topic, topic_hash)
    } else {
        inspect_text(registry, domain, topic_hash)
    }
}

fn topic_filter_hash(topic: Option<&str>) -> Result<Option<TopicHash>, Warning> {
    if let Some(topic_name) = topic {
        validate_topic_name(topic_name)?;
        Ok(Some(stable_topic_hash(topic_name)))
    } else {
        Ok(None)
    }
}

fn inspect_text(
    registry: &DaemonRegistry,
    domain: &str,
    topic_hash: Option<TopicHash>,
) -> Result<String, Warning> {
    let snapshot = registry.snapshot();
    let mut output = format!(
        "domain {domain}: topics={} publishers={} subscribers={} streams={} generation={}",
        snapshot.topic_count,
        snapshot.publisher_count,
        snapshot.subscriber_count,
        snapshot.stream_count,
        snapshot.generation
    );
    for (hash, topic_entry) in registry
        .topics
        .iter()
        .filter(|(hash, _)| topic_hash.is_none_or(|filter_hash| **hash == filter_hash))
    {
        output.push_str(&format!(
            "\ntopic topic_hash={} publishers={} subscribers={} streams={}",
            hash.as_u64(),
            topic_entry.streams.len(),
            topic_entry.subscriber_count,
            topic_entry.streams.len()
        ));
    }
    for (publisher_id, publisher) in registry.publishers.iter().filter(|(_, publisher)| {
        topic_hash.is_none_or(|filter_hash| publisher.topic_hash == filter_hash)
    }) {
        output.push_str(&format!(
            "\npublisher publisher_id={publisher_id} topic_hash={} session_id={}",
            publisher.topic_hash.as_u64(),
            publisher.session_id
        ));
    }
    for (subscriber_id, subscriber) in registry.subscribers.iter().filter(|(_, subscriber)| {
        topic_hash.is_none_or(|filter_hash| subscriber.topic_hash == filter_hash)
    }) {
        output.push_str(&format!(
            "\nsubscriber subscriber_id={subscriber_id} topic_hash={} session_id={}",
            subscriber.topic_hash.as_u64(),
            subscriber.session_id
        ));
    }
    for topic_entry in registry
        .topics
        .iter()
        .filter(|(hash, _)| topic_hash.is_none_or(|filter_hash| **hash == filter_hash))
        .map(|(_, topic_entry)| topic_entry)
    {
        for stream in &topic_entry.streams {
            let stream_snapshot = snapshot_stream_segment(
                &stream.file_descriptor,
                stream.segment_metadata(),
                topic_entry.subscriber_count,
            )?;
            output.push_str(&format!(
                concat!(
                    "\nstream stream_id={} publisher_id={} topic_hash={} generation={} ",
                    "segment_size={} slot_size={} depth={} abi={} flags={} ",
                    "write_sequence={}"
                ),
                stream.metadata.stream_id,
                stream.metadata.publisher_id,
                stream.topic_hash.as_u64(),
                stream.metadata.stream_generation,
                stream.metadata.segment_size,
                stream.metadata.slot_size,
                stream.metadata.depth,
                stream_snapshot.abi_version,
                stream_snapshot.flags,
                stream_snapshot.write_sequence
            ));
        }
    }
    Ok(output)
}

fn inspect_json(
    registry: &DaemonRegistry,
    domain: &str,
    topic: Option<&str>,
    topic_hash: Option<TopicHash>,
) -> Result<String, Warning> {
    let mut output = format!("{{\"domain\":\"{}\"", escape_json_string(domain));
    if let Some(topic_name) = topic {
        output.push_str(&format!(
            ",\"topic\":\"{}\",\"topic_hash\":{}",
            escape_json_string(topic_name),
            topic_hash.map_or(0, TopicHash::as_u64)
        ));
    }
    output.push_str(",\"topics\":[");
    append_topics_json(registry, topic_hash, &mut output);
    output.push_str("],\"publishers\":[");
    append_publishers_json(registry, topic_hash, &mut output);
    output.push_str("],\"subscribers\":[");
    append_subscribers_json(registry, topic_hash, &mut output);
    output.push_str("],\"streams\":[");
    append_streams_json(registry, topic_hash, &mut output)?;
    output.push_str("]}");
    Ok(output)
}

fn append_topics_json(
    registry: &DaemonRegistry,
    topic_hash: Option<TopicHash>,
    output: &mut String,
) {
    let mut first = true;
    for (hash, topic_entry) in registry
        .topics
        .iter()
        .filter(|(hash, _)| topic_hash.is_none_or(|filter_hash| **hash == filter_hash))
    {
        append_separator(&mut first, output);
        output.push_str(&format!(
            "{{\"topic_hash\":{},\"publishers\":{},\"subscribers\":{},\"streams\":{}}}",
            hash.as_u64(),
            topic_entry.streams.len(),
            topic_entry.subscriber_count,
            topic_entry.streams.len()
        ));
    }
}

fn append_publishers_json(
    registry: &DaemonRegistry,
    topic_hash: Option<TopicHash>,
    output: &mut String,
) {
    let mut first = true;
    for (publisher_id, publisher) in registry.publishers.iter().filter(|(_, publisher)| {
        topic_hash.is_none_or(|filter_hash| publisher.topic_hash == filter_hash)
    }) {
        append_separator(&mut first, output);
        output.push_str(&format!(
            "{{\"publisher_id\":{},\"topic_hash\":{},\"session_id\":{}}}",
            publisher_id,
            publisher.topic_hash.as_u64(),
            publisher.session_id
        ));
    }
}

fn append_subscribers_json(
    registry: &DaemonRegistry,
    topic_hash: Option<TopicHash>,
    output: &mut String,
) {
    let mut first = true;
    for (subscriber_id, subscriber) in registry.subscribers.iter().filter(|(_, subscriber)| {
        topic_hash.is_none_or(|filter_hash| subscriber.topic_hash == filter_hash)
    }) {
        append_separator(&mut first, output);
        output.push_str(&format!(
            "{{\"subscriber_id\":{},\"topic_hash\":{},\"session_id\":{}}}",
            subscriber_id,
            subscriber.topic_hash.as_u64(),
            subscriber.session_id
        ));
    }
}

fn append_streams_json(
    registry: &DaemonRegistry,
    topic_hash: Option<TopicHash>,
    output: &mut String,
) -> Result<(), Warning> {
    let mut first = true;
    for topic_entry in registry
        .topics
        .iter()
        .filter(|(hash, _)| topic_hash.is_none_or(|filter_hash| **hash == filter_hash))
        .map(|(_, topic_entry)| topic_entry)
    {
        for stream in &topic_entry.streams {
            let stream_snapshot = snapshot_stream_segment(
                &stream.file_descriptor,
                stream.segment_metadata(),
                topic_entry.subscriber_count,
            )?;
            append_separator(&mut first, output);
            output.push_str(&format!(
                concat!(
                    "{{\"stream_id\":{},\"publisher_id\":{},\"topic_hash\":{},",
                    "\"generation\":{},\"segment_size\":{},\"slot_size\":{},\"depth\":{},",
                    "\"abi\":{},\"flags\":{},\"write_sequence\":{}}}"
                ),
                stream.metadata.stream_id,
                stream.metadata.publisher_id,
                stream.topic_hash.as_u64(),
                stream.metadata.stream_generation,
                stream.metadata.segment_size,
                stream.metadata.slot_size,
                stream.metadata.depth,
                stream_snapshot.abi_version,
                stream_snapshot.flags,
                stream_snapshot.write_sequence
            ));
        }
    }
    Ok(())
}

fn append_separator(first: &mut bool, output: &mut String) {
    if *first {
        *first = false;
    } else {
        output.push(',');
    }
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
