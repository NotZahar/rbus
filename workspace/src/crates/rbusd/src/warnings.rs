use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::stream_segment::snapshot_stream_segment;

use crate::registry::DaemonRegistry;

const COMPONENT_RBUSD: &str = "rbusd";
const MAX_WARNING_SAMPLES: usize = 16;

/// Aggregated warning counters and bounded stream samples
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WarningReport {
    /// Total warning counter across active streams
    pub warnings_total: u64,
    /// Backpressure warnings
    pub backpressure_pinned_slot: u64,
    /// Skipped message warnings
    pub skipped_messages: u64,
    /// Closed stream lifecycle warnings visible in counters
    pub disconnected_streams: u64,
    /// Bounded samples from streams with non-zero warning counters
    pub samples: Vec<WarningSample>,
}

/// Warning counters sampled from one stream
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WarningSample {
    /// Topic hash
    pub topic_hash: u64,
    /// Stream id
    pub stream_id: u64,
    /// Total warnings on stream
    pub warnings_total: u64,
    /// Backpressure warnings on stream
    pub backpressure_pinned_slot: u64,
    /// Skipped message warnings on stream
    pub skipped_messages: u64,
    /// Disconnected stream counter
    pub disconnected_streams: u64,
}

/// Warning model metadata
pub fn warning_report(registry: &DaemonRegistry) -> Result<WarningReport, Warning> {
    let mut report = WarningReport::default();
    for topic_entry in registry.topics.values() {
        for stream in &topic_entry.streams {
            let stream_snapshot = snapshot_stream_segment(
                &stream.file_descriptor,
                stream.segment_metadata(),
                topic_entry.subscriber_count,
            )?;
            add_stream_warning_stats(&mut report, stream_snapshot.stats)?;
            if stream_snapshot.stats.warnings_total != 0
                && report.samples.len() < MAX_WARNING_SAMPLES
            {
                report.samples.push(WarningSample {
                    topic_hash: stream.topic_hash.as_u64(),
                    stream_id: stream.metadata.stream_id,
                    warnings_total: stream_snapshot.stats.warnings_total,
                    backpressure_pinned_slot: stream_snapshot.stats.backpressure_pinned_slot,
                    skipped_messages: stream_snapshot.stats.skipped_messages,
                    disconnected_streams: stream_snapshot.stats.disconnected_streams,
                });
            }
        }
    }
    Ok(report)
}

/// Warning model metadata
pub fn format_warning_report(domain: &str, report: &WarningReport, json: bool) -> String {
    if json {
        warning_report_json(domain, report)
    } else {
        format!(
            concat!(
                "domain {}: warnings_total={} backpressure_pinned_slot={} ",
                "skipped_messages={} disconnected_streams={} samples={}"
            ),
            domain,
            report.warnings_total,
            report.backpressure_pinned_slot,
            report.skipped_messages,
            report.disconnected_streams,
            report.samples.len()
        )
    }
}

fn warning_report_json(domain: &str, report: &WarningReport) -> String {
    let mut response = format!(
        concat!(
            "{{\"domain\":\"{}\",\"warnings_total\":{},",
            "\"backpressure_pinned_slot\":{},\"skipped_messages\":{},",
            "\"disconnected_streams\":{},\"samples\":["
        ),
        escape_json_string(domain),
        report.warnings_total,
        report.backpressure_pinned_slot,
        report.skipped_messages,
        report.disconnected_streams
    );
    let mut first = true;
    for sample in &report.samples {
        append_json_separator(&mut first, &mut response);
        response.push_str(&format!(
            concat!(
                "{{\"topic_hash\":{},\"stream_id\":{},\"warnings_total\":{},",
                "\"backpressure_pinned_slot\":{},\"skipped_messages\":{},",
                "\"disconnected_streams\":{}}}"
            ),
            sample.topic_hash,
            sample.stream_id,
            sample.warnings_total,
            sample.backpressure_pinned_slot,
            sample.skipped_messages,
            sample.disconnected_streams
        ));
    }
    response.push_str("]}");
    response
}

fn add_stream_warning_stats(
    report: &mut WarningReport,
    stats: rbus_core::Stats,
) -> Result<(), Warning> {
    report.warnings_total = checked_add(
        report.warnings_total,
        stats.warnings_total,
        "warnings_total",
    )?;
    report.backpressure_pinned_slot = checked_add(
        report.backpressure_pinned_slot,
        stats.backpressure_pinned_slot,
        "backpressure_pinned_slot",
    )?;
    report.skipped_messages = checked_add(
        report.skipped_messages,
        stats.skipped_messages,
        "skipped_messages",
    )?;
    report.disconnected_streams = checked_add(
        report.disconnected_streams,
        stats.disconnected_streams,
        "disconnected_streams",
    )?;
    Ok(())
}

fn checked_add(left: u64, right: u64, field: &'static str) -> Result<u64, Warning> {
    left.checked_add(right)
        .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUSD, field))
}

fn append_json_separator(first: &mut bool, response: &mut String) {
    if *first {
        *first = false;
    } else {
        response.push(',');
    }
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
