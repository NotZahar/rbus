use crate::metrics::BenchmarkResult;

pub(crate) const CSV_HEADER: &str = concat!(
    "status,scenario,transport,wait_mode,payload_size,publisher_count,subscriber_count,",
    "measured_messages,transport_latency_p50_ns,transport_latency_p90_ns,",
    "transport_latency_p99_ns,transport_latency_p999_ns,transport_latency_p9999_ns,",
    "end_to_end_latency_p50_ns,end_to_end_latency_p90_ns,end_to_end_latency_p99_ns,",
    "end_to_end_latency_p999_ns,end_to_end_latency_p9999_ns,round_trip_p50_ns,",
    "round_trip_p90_ns,round_trip_p99_ns,round_trip_p999_ns,throughput_messages_per_second,",
    "payload_throughput_gib_per_second,cpu_utilization_per_mille,rss_bytes,pss_bytes,",
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
    /// Creates a value
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

    /// Returns a value
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

/// Internal documentation
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmokeReportContext {
    /// Internal documentation
    pub scenario_count: usize,
    /// Benchmark metadata
    pub transport_count: usize,
    /// Payload metadata
    pub payload_size_count: usize,
    /// Benchmark metadata
    pub process_id: u32,
    /// Internal documentation
    pub pinning_enabled: bool,
    /// Publisher runtime metadata
    pub publisher_count: u8,
    /// Subscriber runtime metadata
    pub subscriber_count: u8,
    /// Internal documentation
    pub messages_per_second: u64,
    /// Topic runtime metadata
    pub blocking_wait_wake_latency_ns: u64,
}

/// Benchmark metadata
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
    /// Benchmark metadata
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

    /// Returns a value
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

    /// Returns a value
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
            "\"measured_messages\":{},\"transport_latency_p50_ns\":{},",
            "\"transport_latency_p90_ns\":{},\"transport_latency_p99_ns\":{},",
            "\"transport_latency_p999_ns\":{},\"transport_latency_p9999_ns\":{},",
            "\"end_to_end_latency_p50_ns\":{},\"end_to_end_latency_p90_ns\":{},",
            "\"end_to_end_latency_p99_ns\":{},\"end_to_end_latency_p999_ns\":{},",
            "\"end_to_end_latency_p9999_ns\":{},",
            "\"round_trip_p50_ns\":{},\"round_trip_p90_ns\":{},",
            "\"round_trip_p99_ns\":{},\"round_trip_p999_ns\":{},",
            "\"throughput_messages_per_second\":{:.6},",
            "\"payload_throughput_gib_per_second\":{:.9},",
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
        result.measured_messages,
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
    output.push_str(&format!(
        concat!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6},",
            "{:.9},{},{},{},{},{:.6},{:.6},{},{},{},{},{},{}"
        ),
        status,
        result.scenario,
        result.transport.as_str(),
        result.wait_mode.as_str(),
        result.payload_size,
        result.publisher_count,
        result.subscriber_count,
        result.measured_messages,
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
