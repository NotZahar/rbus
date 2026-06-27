/// Benchmark metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    /// JSON report
    Json,
    /// CSV report
    Csv,
}

impl OutputFormat {
    /// CLI metadata
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "json" => Some(Self::Json),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}
