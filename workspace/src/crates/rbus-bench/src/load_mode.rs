use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LoadMode {
    #[default]
    Saturated,
    Paced,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RateScope {
    #[default]
    PerPublisher,
}

impl LoadMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "saturated" => Some(Self::Saturated),
            "paced" => Some(Self::Paced),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Saturated => "saturated",
            Self::Paced => "paced",
        }
    }
}

impl RateScope {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "per-publisher" => Some(Self::PerPublisher),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PerPublisher => "per-publisher",
        }
    }
}

pub fn parse_positive_rate_hz(value: &str) -> Result<f64, Warning> {
    let rate_hz = value
        .parse::<f64>()
        .map_err(|_| invalid_argument("load_mode::rate_hz"))?;
    if !rate_hz.is_finite() || rate_hz <= 0.0 {
        return Err(invalid_argument("load_mode::rate_hz"));
    }
    Ok(rate_hz)
}

fn invalid_argument(operation: &'static str) -> Warning {
    Warning::new(WarningCode::InvalidConfig, COMPONENT_RBUS_BENCH, operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_mode_parser_accepts_saturated_and_paced() {
        assert_eq!(LoadMode::parse("saturated"), Some(LoadMode::Saturated));
        assert_eq!(LoadMode::parse("paced"), Some(LoadMode::Paced));
    }

    #[test]
    fn paced_mode_requires_positive_rate_hz() {
        assert!(parse_positive_rate_hz("30").is_ok());
        assert!(parse_positive_rate_hz("0").is_err());
        assert!(parse_positive_rate_hz("-1").is_err());
    }

    #[test]
    fn paced_mode_rejects_nan_rate_hz() {
        assert!(parse_positive_rate_hz("NaN").is_err());
    }

    #[test]
    fn paced_mode_rejects_infinite_rate_hz() {
        assert!(parse_positive_rate_hz("inf").is_err());
        assert!(parse_positive_rate_hz("Infinity").is_err());
    }
}
