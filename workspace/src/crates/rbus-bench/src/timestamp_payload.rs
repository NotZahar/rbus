use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const MAX_INLINE_TIMESTAMP_BYTES: usize = 64;
const TIMESTAMP_FIELD_BYTES: usize = core::mem::size_of::<u64>();

pub(crate) const TIMESTAMP_BYTES: usize = TIMESTAMP_FIELD_BYTES * 2;

const _: () = assert!(TIMESTAMP_BYTES <= MAX_INLINE_TIMESTAMP_BYTES);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PayloadTimestamps {
    pub(crate) transport_ns: u64,
    pub(crate) end_to_end_ns: u64,
}

pub(crate) fn encode_timestamps(
    payload: &mut [u8],
    timestamps: PayloadTimestamps,
) -> Result<(), Warning> {
    let target = payload.get_mut(..TIMESTAMP_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUS_BENCH,
            "encode_timestamps",
        )
    })?;
    let (transport_timestamp, end_to_end_timestamp) = target.split_at_mut(TIMESTAMP_FIELD_BYTES);
    transport_timestamp.copy_from_slice(&timestamps.transport_ns.to_ne_bytes());
    end_to_end_timestamp.copy_from_slice(&timestamps.end_to_end_ns.to_ne_bytes());
    Ok(())
}

pub(crate) fn decode_timestamps(payload: &[u8]) -> Result<PayloadTimestamps, Warning> {
    let source = payload.get(..TIMESTAMP_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUS_BENCH,
            "decode_timestamps",
        )
    })?;
    let (transport_timestamp, end_to_end_timestamp) = source.split_at(TIMESTAMP_FIELD_BYTES);
    Ok(PayloadTimestamps {
        transport_ns: timestamp_from_bytes(transport_timestamp),
        end_to_end_ns: timestamp_from_bytes(end_to_end_timestamp),
    })
}

fn timestamp_from_bytes(source: &[u8]) -> u64 {
    let mut bytes = [0_u8; TIMESTAMP_FIELD_BYTES];
    bytes.copy_from_slice(source);
    u64::from_ne_bytes(bytes)
}
