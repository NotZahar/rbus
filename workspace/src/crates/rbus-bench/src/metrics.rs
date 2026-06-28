use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;

const GIB_BYTES: f64 = 1024.0 * 1024.0 * 1024.0;
const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
const PERCENTILE_50_NUMERATOR: u64 = 50;
const PERCENTILE_90_NUMERATOR: u64 = 90;
const PERCENTILE_99_NUMERATOR: u64 = 99;
const PERCENTILE_999_NUMERATOR: u64 = 999;
const PERCENTILE_9999_NUMERATOR: u64 = 9_999;
const PERCENT_DENOMINATOR: u64 = 100;
const PERMILLE_DENOMINATOR: u64 = 1_000;
const PERMYRIAD_DENOMINATOR: u64 = 10_000;

/// Benchmark metadata
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MetricsSnapshot {
    /// Messages per second
    pub messages_per_second: u64,
    /// Topic runtime metadata
    pub blocking_wait_wake_latency_ns: u64,
}

/// Benchmark metadata
#[derive(Clone, Debug, PartialEq)]
pub struct BenchmarkResult {
    /// Internal documentation
    pub scenario: &'static str,
    /// Transport
    pub transport: TransportKind,
    /// Wait mode
    pub wait_mode: WaitMode,
    /// Payload metadata
    pub payload_size: usize,
    /// Publisher runtime metadata
    pub publisher_count: u8,
    /// Subscriber runtime metadata
    pub subscriber_count: u8,
    /// Internal documentation
    pub measured_messages: u64,
    /// p50 transport latency
    pub transport_latency_p50_ns: u64,
    /// p90 transport latency
    pub transport_latency_p90_ns: u64,
    /// p99 transport latency
    pub transport_latency_p99_ns: u64,
    /// p99.9 transport latency
    pub transport_latency_p999_ns: u64,
    /// p99.99 transport latency
    pub transport_latency_p9999_ns: u64,
    /// p50 end-to-end latency
    pub end_to_end_latency_p50_ns: u64,
    /// p90 end-to-end latency
    pub end_to_end_latency_p90_ns: u64,
    /// p99 end-to-end latency
    pub end_to_end_latency_p99_ns: u64,
    /// p99.9 end-to-end latency
    pub end_to_end_latency_p999_ns: u64,
    /// p99.99 end-to-end latency
    pub end_to_end_latency_p9999_ns: u64,
    /// p50 round-trip latency
    pub round_trip_p50_ns: u64,
    /// p90 round-trip latency
    pub round_trip_p90_ns: u64,
    /// p99 round-trip latency
    pub round_trip_p99_ns: u64,
    /// p99.9 round-trip latency
    pub round_trip_p999_ns: u64,
    /// Throughput messages/sec
    pub throughput_messages_per_second: f64,
    /// Effective payload throughput GiB/sec
    pub payload_throughput_gib_per_second: f64,
    /// Benchmark metadata
    pub cpu_utilization_per_mille: u64,
    /// Benchmark metadata
    pub rss_bytes: u64,
    /// Benchmark metadata
    pub pss_bytes: u64,
    /// Benchmark metadata
    pub context_switches: u64,
    /// Syscalls/message
    pub syscalls_per_message: f64,
    /// Allocations/message
    pub allocations_per_message: f64,
    /// Warning count
    pub warning_count: u64,
    /// Skipped messages
    pub skipped_messages: u64,
    /// Pinned-slot backpressure count
    pub backpressure_pinned_slot: u64,
    /// Daemon startup latency
    pub startup_latency_ns: u64,
    /// Attach latency
    pub attach_latency_ns: u64,
    /// Parent orchestration wall time
    pub orchestration_wall_time_ns: u64,
}

/// Percentile snapshot
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LatencyPercentiles {
    /// p50 latency
    pub p50_ns: u64,
    /// p90 latency
    pub p90_ns: u64,
    /// p99 latency
    pub p99_ns: u64,
    /// p99.9 latency
    pub p999_ns: u64,
    /// p99.99 latency
    pub p9999_ns: u64,
}

impl BenchmarkResult {
    /// Benchmark metadata
    pub fn from_samples(
        context: BenchmarkResultContext,
        transport_latency_samples: &mut [u64],
        end_to_end_latency_samples: &mut [u64],
        round_trip_samples: &mut [u64],
    ) -> Self {
        let transport_latency = LatencyPercentiles::from_samples(transport_latency_samples);
        let end_to_end_latency = LatencyPercentiles::from_samples(end_to_end_latency_samples);
        let round_trip = LatencyPercentiles::from_samples(round_trip_samples);
        let throughput =
            throughput_messages_per_second(context.measured_messages, context.measured_window_ns);
        let allocations_per_message =
            per_message(context.allocation_count, context.measured_messages);
        Self {
            scenario: context.scenario,
            transport: context.transport,
            wait_mode: context.wait_mode,
            payload_size: context.payload_size,
            publisher_count: context.publisher_count,
            subscriber_count: context.subscriber_count,
            measured_messages: context.measured_messages,
            transport_latency_p50_ns: transport_latency.p50_ns,
            transport_latency_p90_ns: transport_latency.p90_ns,
            transport_latency_p99_ns: transport_latency.p99_ns,
            transport_latency_p999_ns: transport_latency.p999_ns,
            transport_latency_p9999_ns: transport_latency.p9999_ns,
            end_to_end_latency_p50_ns: end_to_end_latency.p50_ns,
            end_to_end_latency_p90_ns: end_to_end_latency.p90_ns,
            end_to_end_latency_p99_ns: end_to_end_latency.p99_ns,
            end_to_end_latency_p999_ns: end_to_end_latency.p999_ns,
            end_to_end_latency_p9999_ns: end_to_end_latency.p9999_ns,
            round_trip_p50_ns: round_trip.p50_ns,
            round_trip_p90_ns: round_trip.p90_ns,
            round_trip_p99_ns: round_trip.p99_ns,
            round_trip_p999_ns: round_trip.p999_ns,
            throughput_messages_per_second: throughput,
            payload_throughput_gib_per_second: payload_throughput_gib_per_second(
                throughput,
                context.payload_size,
            ),
            cpu_utilization_per_mille: cpu_utilization_per_mille(
                context.cpu_time_ns,
                context.measured_window_ns,
            ),
            rss_bytes: context.rss_bytes,
            pss_bytes: context.pss_bytes,
            context_switches: context.context_switches,
            syscalls_per_message: context.syscalls_per_message,
            allocations_per_message,
            warning_count: context.warning_count,
            skipped_messages: context.skipped_messages,
            backpressure_pinned_slot: context.backpressure_pinned_slot,
            startup_latency_ns: context.startup_latency_ns,
            attach_latency_ns: context.attach_latency_ns,
            orchestration_wall_time_ns: context.orchestration_wall_time_ns,
        }
    }
}

/// Benchmark metadata
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BenchmarkResultContext {
    /// Internal documentation
    pub scenario: &'static str,
    /// Transport
    pub transport: TransportKind,
    /// Wait mode
    pub wait_mode: WaitMode,
    /// Payload metadata
    pub payload_size: usize,
    /// Publisher runtime metadata
    pub publisher_count: u8,
    /// Subscriber runtime metadata
    pub subscriber_count: u8,
    /// Internal documentation
    pub measured_messages: u64,
    /// Internal documentation
    pub measured_window_ns: u64,
    /// Internal documentation
    pub orchestration_wall_time_ns: u64,
    /// Allocation count
    pub allocation_count: u64,
    /// CPU time
    pub cpu_time_ns: u64,
    /// RSS bytes
    pub rss_bytes: u64,
    /// PSS bytes
    pub pss_bytes: u64,
    /// Context switches
    pub context_switches: u64,
    /// Syscalls/message
    pub syscalls_per_message: f64,
    /// Warning count
    pub warning_count: u64,
    /// Skipped messages
    pub skipped_messages: u64,
    /// Backpressure count
    pub backpressure_pinned_slot: u64,
    /// Startup latency
    pub startup_latency_ns: u64,
    /// Attach latency
    pub attach_latency_ns: u64,
}

impl LatencyPercentiles {
    /// Internal documentation
    pub fn from_samples(samples: &mut [u64]) -> Self {
        samples.sort_unstable();
        Self {
            p50_ns: percentile(samples, PERCENTILE_50_NUMERATOR, PERCENT_DENOMINATOR),
            p90_ns: percentile(samples, PERCENTILE_90_NUMERATOR, PERCENT_DENOMINATOR),
            p99_ns: percentile(samples, PERCENTILE_99_NUMERATOR, PERCENT_DENOMINATOR),
            p999_ns: percentile(samples, PERCENTILE_999_NUMERATOR, PERMILLE_DENOMINATOR),
            p9999_ns: percentile(samples, PERCENTILE_9999_NUMERATOR, PERMYRIAD_DENOMINATOR),
        }
    }
}

fn percentile(samples: &[u64], numerator: u64, denominator: u64) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let last_index = samples.len() - 1;
    let last_index_u64 = last_index as u64;
    let rank = last_index_u64
        .saturating_mul(numerator)
        .saturating_add(denominator - 1)
        / denominator;
    samples[rank as usize]
}

fn throughput_messages_per_second(messages: u64, wall_time_ns: u64) -> f64 {
    if wall_time_ns == 0 {
        return 0.0;
    }
    messages as f64 * NANOS_PER_SECOND / wall_time_ns as f64
}

fn payload_throughput_gib_per_second(
    throughput_messages_per_second: f64,
    payload_size: usize,
) -> f64 {
    throughput_messages_per_second * payload_size as f64 / GIB_BYTES
}

fn per_message(count: u64, messages: u64) -> f64 {
    if messages == 0 {
        return 0.0;
    }
    count as f64 / messages as f64
}

fn cpu_utilization_per_mille(cpu_time_ns: u64, wall_time_ns: u64) -> u64 {
    if wall_time_ns == 0 {
        return 0;
    }
    cpu_time_ns.saturating_mul(PERMILLE_DENOMINATOR) / wall_time_ns
}
