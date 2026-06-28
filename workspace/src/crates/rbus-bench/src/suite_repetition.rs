use crate::metrics::BenchmarkResult;

pub(crate) fn aggregate_repeated_results(results: &mut [BenchmarkResult]) -> BenchmarkResult {
    let first = results[0].clone();
    BenchmarkResult {
        scenario: first.scenario,
        transport: first.transport,
        wait_mode: first.wait_mode,
        payload_size: first.payload_size,
        publisher_count: first.publisher_count,
        subscriber_count: first.subscriber_count,
        measured_messages: results.iter().map(|result| result.measured_messages).sum(),
        latency_p50_ns: median_u64(results, |result| result.latency_p50_ns),
        latency_p90_ns: median_u64(results, |result| result.latency_p90_ns),
        latency_p99_ns: median_u64(results, |result| result.latency_p99_ns),
        latency_p999_ns: median_u64(results, |result| result.latency_p999_ns),
        latency_p9999_ns: median_u64(results, |result| result.latency_p9999_ns),
        round_trip_p50_ns: median_u64(results, |result| result.round_trip_p50_ns),
        round_trip_p90_ns: median_u64(results, |result| result.round_trip_p90_ns),
        round_trip_p99_ns: median_u64(results, |result| result.round_trip_p99_ns),
        round_trip_p999_ns: median_u64(results, |result| result.round_trip_p999_ns),
        throughput_messages_per_second: median_f64(results, |result| {
            result.throughput_messages_per_second
        }),
        payload_throughput_gib_per_second: median_f64(results, |result| {
            result.payload_throughput_gib_per_second
        }),
        cpu_utilization_per_mille: median_u64(results, |result| result.cpu_utilization_per_mille),
        rss_bytes: median_u64(results, |result| result.rss_bytes),
        pss_bytes: median_u64(results, |result| result.pss_bytes),
        context_switches: median_u64(results, |result| result.context_switches),
        syscalls_per_message: median_f64(results, |result| result.syscalls_per_message),
        allocations_per_message: median_f64(results, |result| result.allocations_per_message),
        warning_count: results.iter().map(|result| result.warning_count).sum(),
        skipped_messages: results.iter().map(|result| result.skipped_messages).sum(),
        backpressure_pinned_slot: results
            .iter()
            .map(|result| result.backpressure_pinned_slot)
            .sum(),
        startup_latency_ns: median_u64(results, |result| result.startup_latency_ns),
        attach_latency_ns: median_u64(results, |result| result.attach_latency_ns),
    }
}

fn median_u64<F>(results: &[BenchmarkResult], value: F) -> u64
where
    F: Fn(&BenchmarkResult) -> u64,
{
    let mut values: Vec<u64> = results.iter().map(value).collect();
    values.sort_unstable();
    values[values.len() / 2]
}

fn median_f64<F>(results: &[BenchmarkResult], value: F) -> f64
where
    F: Fn(&BenchmarkResult) -> f64,
{
    let mut values: Vec<f64> = results.iter().map(value).collect();
    values.sort_by(|left, right| {
        left.partial_cmp(right)
            .expect("benchmark metrics should be finite")
    });
    values[values.len() / 2]
}

#[cfg(test)]
mod tests {
    use crate::transport_kind::TransportKind;
    use crate::wait_mode::WaitMode;

    use super::*;

    #[test]
    fn repeated_results_use_median_metrics_and_sum_counters() {
        let mut results = [
            result(100, 10.0, 1, 1),
            result(300, 30.0, 2, 3),
            result(200, 20.0, 4, 5),
        ];

        let aggregate = aggregate_repeated_results(&mut results);

        assert_eq!(aggregate.measured_messages, 12);
        assert_eq!(aggregate.latency_p99_ns, 200);
        assert_eq!(aggregate.throughput_messages_per_second, 20.0);
        assert_eq!(aggregate.warning_count, 7);
        assert_eq!(aggregate.skipped_messages, 9);
    }

    fn result(
        latency_p99_ns: u64,
        throughput_messages_per_second: f64,
        warning_count: u64,
        skipped_messages: u64,
    ) -> BenchmarkResult {
        BenchmarkResult {
            scenario: "one-publisher-one-subscriber",
            transport: TransportKind::RbusLoaned,
            wait_mode: WaitMode::BusyPoll,
            payload_size: 64,
            publisher_count: 1,
            subscriber_count: 1,
            measured_messages: 4,
            latency_p50_ns: latency_p99_ns,
            latency_p90_ns: latency_p99_ns,
            latency_p99_ns,
            latency_p999_ns: latency_p99_ns,
            latency_p9999_ns: latency_p99_ns,
            round_trip_p50_ns: 0,
            round_trip_p90_ns: 0,
            round_trip_p99_ns: 0,
            round_trip_p999_ns: 0,
            throughput_messages_per_second,
            payload_throughput_gib_per_second: 0.0,
            cpu_utilization_per_mille: 0,
            rss_bytes: 0,
            pss_bytes: 0,
            context_switches: 0,
            syscalls_per_message: 0.0,
            allocations_per_message: 0.0,
            warning_count,
            skipped_messages,
            backpressure_pinned_slot: 0,
            startup_latency_ns: 0,
            attach_latency_ns: 0,
        }
    }
}
