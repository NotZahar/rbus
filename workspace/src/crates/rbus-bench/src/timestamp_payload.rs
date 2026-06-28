use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const MAX_INLINE_TIMESTAMP_BYTES: usize = 64;

pub(crate) const TIMESTAMP_BYTES: usize = core::mem::size_of::<u64>();

const _: () = assert!(TIMESTAMP_BYTES <= MAX_INLINE_TIMESTAMP_BYTES);

pub(crate) fn encode_timestamp(payload: &mut [u8], timestamp_ns: u64) -> Result<(), Warning> {
    let target = payload.get_mut(..TIMESTAMP_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUS_BENCH,
            "encode_timestamp",
        )
    })?;
    target.copy_from_slice(&timestamp_ns.to_ne_bytes());
    Ok(())
}

pub(crate) fn decode_timestamp(payload: &[u8]) -> Result<u64, Warning> {
    let source = payload.get(..TIMESTAMP_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUS_BENCH,
            "decode_timestamp",
        )
    })?;
    let mut bytes = [0_u8; TIMESTAMP_BYTES];
    bytes.copy_from_slice(source);
    Ok(u64::from_ne_bytes(bytes))
}
