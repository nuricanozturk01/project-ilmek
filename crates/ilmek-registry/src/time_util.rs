use time::OffsetDateTime;

pub(crate) fn datetime_from_ms(ms: u64) -> OffsetDateTime {
    let nanos = i128::from(ms) * 1_000_000;
    OffsetDateTime::from_unix_timestamp_nanos(nanos).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}
