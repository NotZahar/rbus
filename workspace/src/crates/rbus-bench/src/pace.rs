use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::system_metrics::monotonic_time_ns;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
const MILLIS_PER_SECOND: f64 = 1_000.0;
const MICROS_TO_NANOS: u64 = 1_000;
const MINIMUM_PACED_MESSAGE_COUNT: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaceSchedule {
    start_ns: u64,
    rate_hz: f64,
}

impl PaceSchedule {
    pub const fn new(start_ns: u64, rate_hz: f64) -> Self {
        Self { start_ns, rate_hz }
    }

    pub fn deadline_ns(self, message_index: u32) -> Result<u64, Warning> {
        let offset_ns = (f64::from(message_index) * NANOS_PER_SECOND / self.rate_hz).round();
        if !offset_ns.is_finite() || offset_ns < 0.0 || offset_ns > u64::MAX as f64 {
            return Err(invalid_argument("PaceSchedule::deadline_ns"));
        }
        self.start_ns
            .checked_add(offset_ns as u64)
            .ok_or_else(|| invalid_argument("PaceSchedule::deadline_ns"))
    }

    pub fn period_ns(self) -> Result<u64, Warning> {
        let period_ns = (NANOS_PER_SECOND / self.rate_hz).round();
        if !period_ns.is_finite() || period_ns <= 0.0 || period_ns > u64::MAX as f64 {
            return Err(invalid_argument("PaceSchedule::period_ns"));
        }
        Ok(period_ns as u64)
    }
}

pub fn messages_from_duration_and_rate(duration_ms: u64, rate_hz: f64) -> Result<u32, Warning> {
    let messages = ((duration_ms as f64 / MILLIS_PER_SECOND) * rate_hz).floor();
    if !messages.is_finite() || messages < f64::from(MINIMUM_PACED_MESSAGE_COUNT) {
        return Err(invalid_argument("messages_from_duration_and_rate"));
    }
    if messages > f64::from(u32::MAX) {
        return Err(invalid_argument("messages_from_duration_and_rate"));
    }
    Ok(messages as u32)
}

pub fn spin_tail_ns_from_us(spin_tail_us: u64) -> Result<u64, Warning> {
    spin_tail_us
        .checked_mul(MICROS_TO_NANOS)
        .ok_or_else(|| invalid_argument("spin_tail_ns_from_us"))
}

pub fn wait_until(deadline_ns: u64, spin_tail_ns: u64) -> Result<u64, Warning> {
    loop {
        let now_ns = monotonic_time_ns()?;
        if now_ns >= deadline_ns {
            return Ok(now_ns.saturating_sub(deadline_ns));
        }
        let remaining_ns = deadline_ns - now_ns;
        if remaining_ns <= spin_tail_ns {
            core::hint::spin_loop();
        } else {
            let sleep_ns = remaining_ns - spin_tail_ns;
            std::thread::sleep(Duration::from_nanos(sleep_ns));
        }
    }
}

fn invalid_argument(operation: &'static str) -> Warning {
    Warning::new(WarningCode::InvalidConfig, COMPONENT_RBUS_BENCH, operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_START_NS: u64 = 10_000;
    const THIRTY_HZ: f64 = 30.0;
    const ONE_SECOND_MS: u64 = 1_000;

    #[test]
    fn paced_schedule_uses_absolute_deadlines_without_drift() -> Result<(), Warning> {
        let schedule = PaceSchedule::new(TEST_START_NS, THIRTY_HZ);
        let first = schedule.deadline_ns(1)?;
        let second = schedule.deadline_ns(2)?;
        let thirtieth = schedule.deadline_ns(30)?;

        assert_eq!(first, TEST_START_NS + 33_333_333);
        assert_eq!(second, TEST_START_NS + 66_666_667);
        assert_eq!(thirtieth, TEST_START_NS + 1_000_000_000);
        Ok(())
    }

    #[test]
    fn paced_schedule_computes_message_count_from_duration_and_rate() {
        let messages = messages_from_duration_and_rate(ONE_SECOND_MS, THIRTY_HZ);
        assert_eq!(messages, Ok(30));
    }
}
