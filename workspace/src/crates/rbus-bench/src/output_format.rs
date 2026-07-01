/// Output format used for benchmark reports
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    /// JSON report
    Json,
    /// CSV report
    Csv,
}

impl OutputFormat {
    /// Parses an output format CLI value
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "json" => Some(Self::Json),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}
