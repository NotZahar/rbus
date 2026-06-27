use rbus_core::Stats;
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::stream_segment::snapshot_stream_segment;

use crate::registry::DaemonRegistry;

const COMPONENT_RBUSD: &str = "rbusd";

/// Daemon runtime metadata
pub fn daemon_stats_snapshot(registry: &DaemonRegistry) -> Result<Stats, Warning> {
    let mut stats = Stats::default();
    for topic_entry in registry.topics.values() {
        for stream in &topic_entry.streams {
            let stream_snapshot = snapshot_stream_segment(
                &stream.file_descriptor,
                stream.segment_metadata(),
                topic_entry.subscriber_count,
            )?;
            add_stream_stats(&mut stats, stream_snapshot.stats)?;
        }
    }
    stats.connected_streams = registry.stream_count();
    stats.subscriber_count = registry.subscriber_count();
    Ok(stats)
}

/// Control response metadata
pub fn format_stats_response(domain: &str, stats: &Stats, json: bool) -> String {
    if json {
        format!(
            concat!(
                "{{\"domain\":\"{}\",\"published_messages\":{},\"received_messages\":{},",
                "\"skipped_messages\":{},\"backpressure_pinned_slot\":{},",
                "\"warnings_total\":{},\"connected_streams\":{},",
                "\"disconnected_streams\":{},\"current_sequence\":{},",
                "\"subscriber_count\":{},\"publisher_alive\":{},\"attach_count\":{}}}"
            ),
            escape_json_string(domain),
            stats.published_messages,
            stats.received_messages,
            stats.skipped_messages,
            stats.backpressure_pinned_slot,
            stats.warnings_total,
            stats.connected_streams,
            stats.disconnected_streams,
            stats.current_sequence,
            stats.subscriber_count,
            stats.publisher_alive,
            stats.attach_count
        )
    } else {
        format!(
            concat!(
                "domain {}: published={} received={} skipped={} ",
                "backpressure_pinned_slot={} warnings={} connected_streams={} ",
                "disconnected_streams={} current_sequence={} subscribers={} ",
                "publisher_alive={} attach_count={}"
            ),
            domain,
            stats.published_messages,
            stats.received_messages,
            stats.skipped_messages,
            stats.backpressure_pinned_slot,
            stats.warnings_total,
            stats.connected_streams,
            stats.disconnected_streams,
            stats.current_sequence,
            stats.subscriber_count,
            stats.publisher_alive,
            stats.attach_count
        )
    }
}

fn add_stream_stats(stats: &mut Stats, stream_stats: Stats) -> Result<(), Warning> {
    stats.published_messages = checked_add(
        stats.published_messages,
        stream_stats.published_messages,
        "published_messages",
    )?;
    stats.received_messages = checked_add(
        stats.received_messages,
        stream_stats.received_messages,
        "received_messages",
    )?;
    stats.skipped_messages = checked_add(
        stats.skipped_messages,
        stream_stats.skipped_messages,
        "skipped_messages",
    )?;
    stats.backpressure_pinned_slot = checked_add(
        stats.backpressure_pinned_slot,
        stream_stats.backpressure_pinned_slot,
        "backpressure_pinned_slot",
    )?;
    stats.warnings_total = checked_add(
        stats.warnings_total,
        stream_stats.warnings_total,
        "warnings_total",
    )?;
    stats.disconnected_streams = checked_add(
        stats.disconnected_streams,
        stream_stats.disconnected_streams,
        "disconnected_streams",
    )?;
    stats.current_sequence = stats.current_sequence.max(stream_stats.current_sequence);
    stats.publisher_alive |= stream_stats.publisher_alive;
    stats.attach_count = checked_add(
        stats.attach_count,
        stream_stats.attach_count,
        "attach_count",
    )?;
    Ok(())
}

fn checked_add(left: u64, right: u64, field: &'static str) -> Result<u64, Warning> {
    left.checked_add(right)
        .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUSD, field))
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
