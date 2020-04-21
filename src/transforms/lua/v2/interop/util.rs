use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use rlua::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn timestamp_to_table<'a>(ctx: LuaContext<'a>, ts: DateTime<Utc>) -> LuaResult<LuaTable> {
    let table = ctx.create_table()?;
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

pub fn table_is_timestamp<'a>(t: &LuaTable<'a>) -> LuaResult<bool> {
    for &key in &["year", "month", "day", "hour", "min", "sec"] {
        if !t.contains_key(key)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn table_to_timestamp<'a>(t: LuaTable<'a>) -> LuaResult<DateTime<Utc>> {
    let year = t.get("year")?;
    let month = t.get("month")?;
    let day = t.get("day")?;
    let hour = t.get("hour")?;
    let min = t.get("min")?;
    let sec = t.get("sec")?;
    let nano = t.get::<_, Option<u32>>("nanosec")?.unwrap_or(0);
    Ok(Utc.ymd(year, month, day).and_hms_nano(hour, min, sec, nano))
}

pub fn table_to_map<'a, K, V>(t: LuaTable<'a>) -> LuaResult<BTreeMap<K, V>>
where
    K: From<String> + Ord,
    V: FromLua<'a>,
{
    let mut map = BTreeMap::new();
    for pair in t.pairs() {
        let (k, v): (String, V) = pair?;
        map.insert(k.into(), v);
    }
    Ok(map)
}

pub fn table_to_set<'a, T>(t: LuaTable<'a>) -> LuaResult<BTreeSet<T>>
where
    T: FromLua<'a> + Ord,
{
    let mut set = BTreeSet::new();
    for item in t.sequence_values() {
        set.insert(item?);
    }
    Ok(set)
}

pub fn table_is_array<'a>(t: &LuaTable<'a>) -> LuaResult<bool> {
    Ok(t.len()? > 0)
}

pub fn table_to_array<'a, T>(t: LuaTable<'a>) -> LuaResult<Vec<T>>
where
    T: FromLua<'a>,
{
    let mut seq = Vec::new();
    for item in t.sequence_values() {
        let value = item?;
        seq.push(value);
    }
    Ok(seq)
}
