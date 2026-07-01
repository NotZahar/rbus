use crate::metrics::BenchmarkResult;

pub(crate) const CSV_HEADER: &str = concat!(
    "status,scenario,transport,wait_mode,payload_size,publisher_count,subscriber_count,",
    "load_mode,rate_hz,rate_scope,duration_ms,configured_messages_per_publisher,",
    "actual_messages_per_publisher,measured_messages,publish_deadline_miss_count,",
    "publish_lateness_p50_ns,publish_lateness_p90_ns,publish_lateness_p99_ns,",
    "publish_lateness_max_ns,inter_publish_gap_p50_ns,inter_publish_gap_p99_ns,",
    "inter_receive_gap_p50_ns,inter_receive_gap_p99_ns,backlog_metric_available,",
    "ring_backlog_p50_messages,ring_backlog_p90_messages,ring_backlog_p99_messages,",
    "ring_backlog_max_messages,transport_latency_p50_ns,transport_latency_p90_ns,",
    "transport_latency_p99_ns,transport_latency_p999_ns,transport_latency_p9999_ns,",
    "end_to_end_latency_p50_ns,end_to_end_latency_p90_ns,end_to_end_latency_p99_ns,",
    "end_to_end_latency_p999_ns,end_to_end_latency_p9999_ns,round_trip_p50_ns,",
    "round_trip_p90_ns,round_trip_p99_ns,round_trip_p999_ns,throughput_messages_per_second,",
    "payload_throughput_gib_per_second,unique_publish_messages_per_second,",
    "delivered_messages_per_second,actual_payload_write_gib_per_second,",
    "effective_payload_delivery_gib_per_second,cpu_utilization_per_mille,rss_bytes,pss_bytes,",
    "context_switches,syscalls_per_message,allocations_per_message,warning_count,",
    "skipped_messages,backpressure_pinned_slot,startup_latency_ns,attach_latency_ns,",
    "orchestration_wall_time_ns"
);

/// Smoke benchmark report
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmokeReport {
    status: &'static str,
    warning_count: u64,
    scenario_count: usize,
    transport_count: usize,
    payload_size_count: usize,
    process_id: u32,
    pinning_enabled: bool,
    publisher_count: u8,
    subscriber_count: u8,
    messages_per_second: u64,
    blocking_wait_wake_latency_ns: u64,
}

impl SmokeReport {
    /// Creates a smoke report from run status and context
    pub const fn new(
        status: &'static str,
        warning_count: u64,
        context: SmokeReportContext,
    ) -> Self {
        Self {
            status,
            warning_count,
            scenario_count: context.scenario_count,
            transport_count: context.transport_count,
            payload_size_count: context.payload_size_count,
            process_id: context.process_id,
            pinning_enabled: context.pinning_enabled,
            publisher_count: context.publisher_count,
            subscriber_count: context.subscriber_count,
            messages_per_second: context.messages_per_second,
            blocking_wait_wake_latency_ns: context.blocking_wait_wake_latency_ns,
        }
    }

    /// Serializes the smoke report as JSON
    pub fn to_json(self) -> String {
        format!(
            concat!(
                "{{\"status\":\"{}\",\"warning_count\":{},",
                "\"scenario_count\":{},\"transport_count\":{},\"payload_size_count\":{},",
                "\"process_id\":{},\"pinning_enabled\":{},",
                "\"publisher_count\":{},\"subscriber_count\":{},",
                "\"messages_per_second\":{},\"blocking_wait_wake_latency_ns\":{}}}"
            ),
            self.status,
            self.warning_count,
            self.scenario_count,
            self.transport_count,
            self.payload_size_count,
            self.process_id,
            self.pinning_enabled,
            self.publisher_count,
            self.subscriber_count,
            self.messages_per_second,
            self.blocking_wait_wake_latency_ns
        )
    }
}

/// Inputs used to build a smoke report
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmokeReportContext {
    /// Number of benchmark scenarios included
    pub scenario_count: usize,
    /// Number of transports included
    pub transport_count: usize,
    /// Number of payload sizes included
    pub payload_size_count: usize,
    /// Process id that generated the report
    pub process_id: u32,
    /// Whether CPU pinning was enabled
    pub pinning_enabled: bool,
    /// Number of publishers used
    pub publisher_count: u8,
    /// Number of subscribers used
    pub subscriber_count: u8,
    /// Smoke throughput sample
    pub messages_per_second: u64,
    /// Blocking wait wake latency sample
    pub blocking_wait_wake_latency_ns: u64,
}

/// Full suite report with aggregate counts and row results
#[derive(Clone, Debug, PartialEq)]
pub struct SuiteReport {
    status: &'static str,
    gate_status: &'static str,
    scenario_count: usize,
    payload_size_count: usize,
    wait_mode_count: usize,
    transport_count: usize,
    results: Vec<BenchmarkResult>,
}

impl SuiteReport {
    /// Creates a suite report from aggregate counts and row results
    pub fn new(
        status: &'static str,
        gate_status: &'static str,
        scenario_count: usize,
        payload_size_count: usize,
        wait_mode_count: usize,
        transport_count: usize,
        results: Vec<BenchmarkResult>,
    ) -> Self {
        Self {
            status,
            gate_status,
            scenario_count,
            payload_size_count,
            wait_mode_count,
            transport_count,
            results,
        }
    }

    /// Serializes the suite report as JSON
    pub fn to_json(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            concat!(
                "{{\"status\":\"{}\",\"gate_status\":\"{}\",",
                "\"scenario_count\":{},\"payload_size_count\":{},",
                "\"wait_mode_count\":{},\"transport_count\":{},\"result_count\":{},",
                "\"results\":["
            ),
            self.status,
            self.gate_status,
            self.scenario_count,
            self.payload_size_count,
            self.wait_mode_count,
            self.transport_count,
            self.results.len()
        ));
        for (index, result) in self.results.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            push_result_json(&mut output, result);
        }
        output.push_str("]}");
        output
    }

    /// Serializes the suite report as CSV
    pub fn to_csv(&self) -> String {
        let mut output = String::new();
        output.push_str(CSV_HEADER);
        output.push('\n');
        for result in &self.results {
            push_result_csv(&mut output, self.status, result);
            output.push('\n');
        }
        output
    }
}

pub(crate) fn result_csv_line(status: &str, result: &BenchmarkResult) -> String {
    let mut output = String::new();
    push_result_csv(&mut output, status, result);
    output
}

fn push_result_json(output: &mut String, result: &BenchmarkResult) {
    output.push_str(&format!(
        concat!(
            "{{\"scenario\":\"{}\",\"transport\":\"{}\",\"wait_mode\":\"{}\",",
            "\"payload_size\":{},\"publisher_count\":{},\"subscriber_count\":{},",
            "\"load_mode\":\"{}\",\"rate_hz\":{:.9},\"rate_scope\":\"{}\",",
            "\"duration_ms\":{},\"configured_messages_per_publisher\":{},",
            "\"actual_messages_per_publisher\":{},\"measured_messages\":{},",
            "\"publish_deadline_miss_count\":{},",
            "\"publish_lateness_p50_ns\":{},\"publish_lateness_p90_ns\":{},",
            "\"publish_lateness_p99_ns\":{},\"publish_lateness_max_ns\":{},",
            "\"inter_publish_gap_p50_ns\":{},\"inter_publish_gap_p99_ns\":{},",
            "\"inter_receive_gap_p50_ns\":{},\"inter_receive_gap_p99_ns\":{},",
            "\"backlog_metric_available\":{},",
            "\"ring_backlog_p50_messages\":{},\"ring_backlog_p90_messages\":{},",
            "\"ring_backlog_p99_messages\":{},\"ring_backlog_max_messages\":{},",
            "\"transport_latency_p50_ns\":{},",
            "\"transport_latency_p90_ns\":{},\"transport_latency_p99_ns\":{},",
            "\"transport_latency_p999_ns\":{},\"transport_latency_p9999_ns\":{},",
            "\"end_to_end_latency_p50_ns\":{},\"end_to_end_latency_p90_ns\":{},",
            "\"end_to_end_latency_p99_ns\":{},\"end_to_end_latency_p999_ns\":{},",
            "\"end_to_end_latency_p9999_ns\":{},",
            "\"round_trip_p50_ns\":{},\"round_trip_p90_ns\":{},",
            "\"round_trip_p99_ns\":{},\"round_trip_p999_ns\":{},",
            "\"throughput_messages_per_second\":{:.6},",
            "\"payload_throughput_gib_per_second\":{:.9},",
            "\"unique_publish_messages_per_second\":{:.6},",
            "\"delivered_messages_per_second\":{:.6},",
            "\"actual_payload_write_gib_per_second\":{:.9},",
            "\"effective_payload_delivery_gib_per_second\":{:.9},",
            "\"cpu_utilization_per_mille\":{},\"rss_bytes\":{},\"pss_bytes\":{},",
            "\"context_switches\":{},\"syscalls_per_message\":{:.6},",
            "\"allocations_per_message\":{:.6},\"warning_count\":{},",
            "\"skipped_messages\":{},\"backpressure_pinned_slot\":{},",
            "\"startup_latency_ns\":{},\"attach_latency_ns\":{},",
            "\"orchestration_wall_time_ns\":{}}}"
        ),
        result.scenario,
        result.transport.as_str(),
        result.wait_mode.as_str(),
        result.payload_size,
        result.publisher_count,
        result.subscriber_count,
        result.load_mode.as_str(),
        result.rate_hz,
        result.rate_scope.as_str(),
        result.duration_ms,
        result.configured_messages_per_publisher,
        result.actual_messages_per_publisher,
        result.measured_messages,
        result.publish_deadline_miss_count,
        result.publish_lateness_p50_ns,
        result.publish_lateness_p90_ns,
        result.publish_lateness_p99_ns,
        result.publish_lateness_max_ns,
        result.inter_publish_gap_p50_ns,
        result.inter_publish_gap_p99_ns,
        result.inter_receive_gap_p50_ns,
        result.inter_receive_gap_p99_ns,
        result.backlog_metric_available,
        result.ring_backlog_p50_messages,
        result.ring_backlog_p90_messages,
        result.ring_backlog_p99_messages,
        result.ring_backlog_max_messages,
        result.transport_latency_p50_ns,
        result.transport_latency_p90_ns,
        result.transport_latency_p99_ns,
        result.transport_latency_p999_ns,
        result.transport_latency_p9999_ns,
        result.end_to_end_latency_p50_ns,
        result.end_to_end_latency_p90_ns,
        result.end_to_end_latency_p99_ns,
        result.end_to_end_latency_p999_ns,
        result.end_to_end_latency_p9999_ns,
        result.round_trip_p50_ns,
        result.round_trip_p90_ns,
        result.round_trip_p99_ns,
        result.round_trip_p999_ns,
        result.throughput_messages_per_second,
        result.payload_throughput_gib_per_second,
        result.unique_publish_messages_per_second,
        result.delivered_messages_per_second,
        result.actual_payload_write_gib_per_second,
        result.effective_payload_delivery_gib_per_second,
        result.cpu_utilization_per_mille,
        result.rss_bytes,
        result.pss_bytes,
        result.context_switches,
        result.syscalls_per_message,
        result.allocations_per_message,
        result.warning_count,
        result.skipped_messages,
        result.backpressure_pinned_slot,
        result.startup_latency_ns,
        result.attach_latency_ns,
        result.orchestration_wall_time_ns
    ));
}

fn push_result_csv(output: &mut String, status: &str, result: &BenchmarkResult) {
    let fields = [
        status.to_owned(),
        result.scenario.to_owned(),
        result.transport.as_str().to_owned(),
        result.wait_mode.as_str().to_owned(),
        result.payload_size.to_string(),
        result.publisher_count.to_string(),
        result.subscriber_count.to_string(),
        result.load_mode.as_str().to_owned(),
        format!("{:.9}", result.rate_hz),
        result.rate_scope.as_str().to_owned(),
        result.duration_ms.to_string(),
        result.configured_messages_per_publisher.to_string(),
        result.actual_messages_per_publisher.to_string(),
        result.measured_messages.to_string(),
        result.publish_deadline_miss_count.to_string(),
        result.publish_lateness_p50_ns.to_string(),
        result.publish_lateness_p90_ns.to_string(),
        result.publish_lateness_p99_ns.to_string(),
        result.publish_lateness_max_ns.to_string(),
        result.inter_publish_gap_p50_ns.to_string(),
        result.inter_publish_gap_p99_ns.to_string(),
        result.inter_receive_gap_p50_ns.to_string(),
        result.inter_receive_gap_p99_ns.to_string(),
        result.backlog_metric_available.to_string(),
        result.ring_backlog_p50_messages.to_string(),
        result.ring_backlog_p90_messages.to_string(),
        result.ring_backlog_p99_messages.to_string(),
        result.ring_backlog_max_messages.to_string(),
        result.transport_latency_p50_ns.to_string(),
        result.transport_latency_p90_ns.to_string(),
        result.transport_latency_p99_ns.to_string(),
        result.transport_latency_p999_ns.to_string(),
        result.transport_latency_p9999_ns.to_string(),
        result.end_to_end_latency_p50_ns.to_string(),
        result.end_to_end_latency_p90_ns.to_string(),
        result.end_to_end_latency_p99_ns.to_string(),
        result.end_to_end_latency_p999_ns.to_string(),
        result.end_to_end_latency_p9999_ns.to_string(),
        result.round_trip_p50_ns.to_string(),
        result.round_trip_p90_ns.to_string(),
        result.round_trip_p99_ns.to_string(),
        result.round_trip_p999_ns.to_string(),
        format!("{:.6}", result.throughput_messages_per_second),
        format!("{:.9}", result.payload_throughput_gib_per_second),
        format!("{:.6}", result.unique_publish_messages_per_second),
        format!("{:.6}", result.delivered_messages_per_second),
        format!("{:.9}", result.actual_payload_write_gib_per_second),
        format!("{:.9}", result.effective_payload_delivery_gib_per_second),
        result.cpu_utilization_per_mille.to_string(),
        result.rss_bytes.to_string(),
        result.pss_bytes.to_string(),
        result.context_switches.to_string(),
        format!("{:.6}", result.syscalls_per_message),
        format!("{:.6}", result.allocations_per_message),
        result.warning_count.to_string(),
        result.skipped_messages.to_string(),
        result.backpressure_pinned_slot.to_string(),
        result.startup_latency_ns.to_string(),
        result.attach_latency_ns.to_string(),
        result.orchestration_wall_time_ns.to_string(),
    ];
    output.push_str(&fields.join(","));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paced_report_contains_required_fields() {
        for field in [
            "load_mode",
            "rate_hz",
            "rate_scope",
            "duration_ms",
            "configured_messages_per_publisher",
            "actual_messages_per_publisher",
            "publish_deadline_miss_count",
            "publish_lateness_p99_ns",
            "inter_publish_gap_p99_ns",
            "inter_receive_gap_p99_ns",
            "backlog_metric_available",
            "ring_backlog_p99_messages",
            "transport_latency_p99_ns",
            "end_to_end_latency_p99_ns",
            "unique_publish_messages_per_second",
            "delivered_messages_per_second",
            "actual_payload_write_gib_per_second",
            "effective_payload_delivery_gib_per_second",
        ] {
            assert!(CSV_HEADER.contains(field));
        }
    }
}
