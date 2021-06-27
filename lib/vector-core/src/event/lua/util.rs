use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use mlua::prelude::*;

/// Convert a `DateTime<Utc>` to a `LuaTable`
///
/// # Errors
///
/// This function will fail insertion into the table fails.
pub fn timestamp_to_table(lua: &Lua, ts: DateTime<Utc>) -> LuaResult<LuaTable> {
    let table = lua.create_table()?;
    table.set("year", ts.year())?;
    table.set("month", ts.month())?;
    table.set("day", ts.day())?;
    table.set("hour", ts.hour())?;
    table.set("min", ts.minute())?;
    table.set("sec", ts.second())?;
    table.set("nanosec", ts.nanosecond())?;
    table.set("yday", ts.ordinal())?;
    table.set("wday", ts.weekday().number_from_sunday())?;
    table.set("isdst", false)?;

    Ok(table)
}

/// Determines if a `LuaTable` is a timestamp.
///
/// # Errors
///
/// This function will fail if the table is malformed.
pub fn table_is_timestamp(t: &LuaTable<'_>) -> LuaResult<bool> {
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
#[allow(clippy::needless_pass_by_value)] // constrained by mlua types
pub fn table_to_timestamp(t: LuaTable<'_>) -> LuaResult<DateTime<Utc>> {
    let year = t.get("year")?;
    let month = t.get("month")?;
    let day = t.get("day")?;
    let hour = t.get("hour")?;
    let min = t.get("min")?;
    let sec = t.get("sec")?;
    let nano = t.get::<_, Option<u32>>("nanosec")?.unwrap_or(0);
    Ok(Utc.ymd(year, month, day).and_hms_nano(hour, min, sec, nano))
}
