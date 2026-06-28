use crate::worker::WorkerReport;

#[derive(Clone, Debug)]
pub(crate) struct ParsedWorkerOutput {
    pub(crate) report: Option<WorkerReport>,
    pub(crate) transport_latencies: Vec<u64>,
    pub(crate) end_to_end_latencies: Vec<u64>,
    pub(crate) round_trip_latencies: Vec<u64>,
}
