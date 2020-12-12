use crate::value::Value;
use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use rlua::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

impl<'a> ToLua<'a> for Value {
    fn to_lua(self, ctx: LuaContext<'a>) -> LuaResult<LuaValue> {
        match self {
            Value::Bytes(b) => ctx.create_string(b.as_ref()).map(LuaValue::String),
            Value::Integer(i) => Ok(LuaValue::Integer(i)),
            Value::Float(f) => Ok(LuaValue::Number(f)),
            Value::Boolean(b) => Ok(LuaValue::Boolean(b)),
            Value::Timestamp(t) => timestamp_to_table(ctx, t).map(LuaValue::Table),
            Value::Map(m) => ctx
                .create_table_from(m.into_iter().map(|(k, v)| (k, v)))
                .map(LuaValue::Table),
            Value::Array(a) => ctx.create_sequence_from(a.into_iter()).map(LuaValue::Table),
            Value::Null => ctx.create_string("").map(LuaValue::String),
        }
    }
}

impl<'a> FromLua<'a> for Value {
    fn from_lua(value: LuaValue<'a>, _: LuaContext<'a>) -> LuaResult<Self> {
        match value {
            LuaValue::String(s) => Ok(Value::Bytes(Vec::from(s.as_bytes()).into())),
            LuaValue::Integer(i) => Ok(Value::Integer(i)),
            LuaValue::Number(f) => Ok(Value::Float(f)),
            LuaValue::Boolean(b) => Ok(Value::Boolean(b)),
            LuaValue::Table(t) => {
                if table_is_array(&t)? {
                    table_to_array(t).map(Value::Array)
                } else if table_is_timestamp(&t)? {
                    table_to_timestamp(t).map(Value::Timestamp)
                } else {
                    table_to_map(t).map(Value::Map)
                }
            }
            other => Err(rlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: "Value",
                message: Some("Unsupported Lua type".to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn from_lua() {
        let pairs = vec![
            ("'⍺βγ'", Value::Bytes("⍺βγ".into())),
            ("123", Value::Integer(123)),
            ("3.14159265359", Value::Float(3.14159265359)),
            ("true", Value::Boolean(true)),
            (
                "{ x = 1, y = '2', nested = { other = 2.718281828 } }",
                Value::Map(
                    vec![
                        ("x".into(), 1.into()),
                        ("y".into(), "2".into()),
                        (
                            "nested".into(),
                            Value::Map(
                                vec![("other".into(), 2.718281828.into())]
                                    .into_iter()
                                    .collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            (
                "{1, '2', 0.57721566}",
                Value::Array(vec![1.into(), "2".into(), 0.57721566.into()]),
            ),
            (
                "os.date('!*t', 1584297428)",
                Value::Timestamp(Utc.ymd(2020, 3, 15).and_hms(18, 37, 8)),
            ),
            (
                "{year=2020, month=3, day=15, hour=18, min=37, sec=8}",
                Value::Timestamp(Utc.ymd(2020, 3, 15).and_hms(18, 37, 8)),
            ),
            (
                "{year=2020, month=3, day=15, hour=18, min=37, sec=8, nanosec=666666666}",
                Value::Timestamp(Utc.ymd(2020, 3, 15).and_hms_nano(18, 37, 8, 666666666)),
            ),
        ];

        Lua::new().context(move |ctx| {
            for (expression, expected) in pairs.into_iter() {
                let value: Value = ctx.load(expression).eval().unwrap();
                assert_eq!(value, expected, "expression: {:?}", expression);
            }
        });
    }

    #[test]
    fn to_lua() {
        let pairs = vec![
            (
                Value::Bytes("⍺βγ".into()),
                r#"
                function (value)
                    return value == '⍺βγ'
                end
                "#,
            ),
            (
                Value::Integer(123),
                r#"
                function (value)
                    return value == 123
                end
                "#,
            ),
            (
                Value::Float(3.14159265359),
                r#"
                function (value)
                    return value == 3.14159265359
                end
                "#,
            ),
            (
                Value::Null,
                r#"
                function (value)
                    return value == ''
                end
                "#,
            ),
            (
                Value::Map(
                    vec![
                        ("x".into(), 1.into()),
                        ("y".into(), "2".into()),
                        (
                            "nested".into(),
                            Value::Map(
                                vec![("other".into(), 2.718281828.into())]
                                    .into_iter()
                                    .collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
                r#"
                function (value)
                    return value.x == 1 and
                        value['y'] == '2' and
                        value.nested.other == 2.718281828
                end
                "#,
            ),
            (
                Value::Array(vec![1.into(), "2".into(), 0.57721566.into()]),
                r#"
                function (value)
                    return value[1] == 1 and
                        value[2] == '2' and
                        value[3] == 0.57721566
                end
                "#,
            ),
            (
                Value::Timestamp(Utc.ymd(2020, 3, 15).and_hms_nano(18, 37, 8, 666666666)),
                r#"
                function (value)
                    local expected = os.date("!*t", 1584297428)
                    expected.nanosec = 666666666

                    return os.time(value) == os.time(expected) and
                        value.nanosec == expected.nanosec and
                        value.yday == expected.yday and
                        value.wday == expected.wday and
                        value.isdst == expected.isdst
                end
                "#,
            ),
        ];

        Lua::new().context(move |ctx| {
            for (value, test_src) in pairs.into_iter() {
                let test_fn: LuaFunction = ctx.load(test_src).eval().unwrap_or_else(|_| {
                    panic!("Failed to load {} for value {:?}", test_src, value)
                });
                assert!(
                    test_fn
                        .call::<_, bool>(value.clone())
                        .unwrap_or_else(|_| panic!(
                            "Failed to call {} for value {:?}",
                            test_src, value
                        )),
                    "Test function: {}, value: {:?}",
                    test_src,
                    value
                );
            }
        });
    }
}

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

pub fn table_is_timestamp(t: &LuaTable<'_>) -> LuaResult<bool> {
    for &key in &["year", "month", "day", "hour", "min", "sec"] {
        if !t.contains_key(key)? {
            return Ok(false);
        }
    }
    Ok(true)
}

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
