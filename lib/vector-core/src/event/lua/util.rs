use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use mlua::prelude::*;

/// An integer read from Lua that truncates fractional numbers toward zero.
///
/// mlua 0.12 rejects non-integral numbers when converting to Rust integer types under Lua 5.4,
/// while mlua 0.11 truncated them. Scripts commonly compute integer fields with float arithmetic
/// (for example `interval_ms = 1000 / 3`), so this wrapper keeps the previous truncating behavior.
/// Out-of-range and non-numeric values still fail the conversion.
pub struct LuaInt<T>(pub T);

impl<T: FromLua> FromLua for LuaInt<T> {
    fn from_lua(value: LuaValue, lua: &Lua) -> LuaResult<Self> {
        let value = match value {
            LuaValue::Number(n) => LuaValue::Number(n.trunc()),
            LuaValue::String(_) => {
                if let Some(i) = lua.coerce_integer(value.clone())? {
                    LuaValue::Integer(i)
                } else if let Some(n) = lua.coerce_number(value.clone())? {
                    LuaValue::Number(n.trunc())
                } else {
                    value
                }
            }
            other => other,
        };
        T::from_lua(value, lua).map(Self)
    }
}

/// Convert a `DateTime<Utc>` to a `LuaTable`
///
/// # Errors
///
/// This function will fail insertion into the table fails.
pub fn timestamp_to_table(lua: &Lua, ts: DateTime<Utc>) -> LuaResult<LuaTable> {
    let table = lua.create_table()?;
    table.raw_set("year", ts.year())?;
    table.raw_set("month", ts.month())?;
    table.raw_set("day", ts.day())?;
    table.raw_set("hour", ts.hour())?;
    table.raw_set("min", ts.minute())?;
    table.raw_set("sec", ts.second())?;
    table.raw_set("nanosec", ts.nanosecond())?;
    table.raw_set("yday", ts.ordinal())?;
    table.raw_set("wday", ts.weekday().number_from_sunday())?;
    table.raw_set("isdst", false)?;

    Ok(table)
}

/// Determines if a `LuaTable` is a timestamp.
///
/// # Errors
///
/// This function will fail if the table is malformed.
pub fn table_is_timestamp(t: &LuaTable) -> LuaResult<bool> {
    for &key in &["year", "month", "day", "hour", "min", "sec"] {
        if !t.contains_key(key)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Convert a `LuaTable` to a `DateTime<Utc>`
///
/// # Errors
///
/// This function will fail if the table is malformed.
///
/// # Panics
///
/// Panics if the resulting timestamp is invalid.
#[allow(clippy::needless_pass_by_value)] // constrained by mlua types
pub fn table_to_timestamp(t: LuaTable) -> LuaResult<DateTime<Utc>> {
    let LuaInt(year) = t.raw_get("year")?;
    let LuaInt(month) = t.raw_get("month")?;
    let LuaInt(day) = t.raw_get("day")?;
    let LuaInt(hour) = t.raw_get("hour")?;
    let LuaInt(min) = t.raw_get("min")?;
    let LuaInt(sec) = t.raw_get("sec")?;
    let nano = t
        .raw_get::<Option<LuaInt<u32>>>("nanosec")?
        .map_or(0, |LuaInt(nano)| nano);
    Ok(Utc
        .with_ymd_and_hms(year, month, day, hour, min, sec)
        .single()
        .and_then(|t| t.with_nanosecond(nano))
        .expect("invalid timestamp"))
}
