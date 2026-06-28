use crate::worker::WorkerReport;

#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedWorkerOutput {
    pub(crate) report: Option<WorkerReport>,
    pub(crate) transport_latencies: Vec<u64>,
    pub(crate) end_to_end_latencies: Vec<u64>,
    pub(crate) round_trip_latencies: Vec<u64>,
    pub(crate) publish_lateness: Vec<u64>,
    pub(crate) inter_publish_gaps: Vec<u64>,
    pub(crate) inter_receive_gaps: Vec<u64>,
    pub(crate) ring_backlog_messages: Vec<u64>,
}
