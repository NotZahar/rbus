use crate::worker::WorkerReport;

#[derive(Clone, Debug)]
pub(crate) struct ParsedWorkerOutput {
    pub(crate) report: Option<WorkerReport>,
    pub(crate) latencies: Vec<u64>,
}

impl ParsedWorkerOutput {
    pub(crate) fn clone_report_only(&self) -> Self {
        Self {
            report: self.report,
            latencies: Vec::new(),
        }
    }
}
