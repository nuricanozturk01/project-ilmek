use rusqlite::ToSql;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef};

/// A `u64` stored as SQLite INTEGER; values above `i64::MAX` are rejected, never wrapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unsigned(pub u64);

impl ToSql for Unsigned {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        i64::try_from(self.0)
            .map(|value| ToSqlOutput::Owned(Value::Integer(value)))
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    }
}

impl FromSql for Unsigned {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let signed = i64::column_result(value)?;
        u64::try_from(signed)
            .map(Self)
            .map_err(|_| FromSqlError::OutOfRange(signed))
    }
}
