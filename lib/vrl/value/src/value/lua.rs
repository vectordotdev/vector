use mlua::prelude::LuaResult;
use mlua::{FromLua, Lua, ToLua, Value as LuaValue};
use ordered_float::NotNan;

use crate::value::Value;

impl<'a> ToLua<'a> for Value {
    #![allow(clippy::wrong_self_convention)] // this trait is defined by mlua
    fn to_lua(self, lua: &'a Lua) -> LuaResult<LuaValue<'_>> {
        match self {
            Self::Bytes(b) => lua.create_string(b.as_ref()).map(LuaValue::String),
            Self::Regex(regex) => lua
                .create_string(regex.as_bytes_slice())
                .map(LuaValue::String),
            Self::Integer(i) => Ok(LuaValue::Integer(i)),
            Self::Float(f) => Ok(LuaValue::Number(f.into_inner())),
            Self::Boolean(b) => Ok(LuaValue::Boolean(b)),
            Self::Timestamp(t) => timestamp_to_table(lua, t).map(LuaValue::Table),
            Self::Object(m) => lua.create_table_from(m.into_iter()).map(LuaValue::Table),
            Self::Array(a) => lua.create_sequence_from(a.into_iter()).map(LuaValue::Table),
            Self::Null => lua.create_string("").map(LuaValue::String),
        }
    }
}

impl<'a> FromLua<'a> for Value {
    fn from_lua(value: LuaValue<'a>, lua: &'a Lua) -> LuaResult<Self> {
        match value {
            LuaValue::String(s) => Ok(Self::Bytes(Vec::from(s.as_bytes()).into())),
            LuaValue::Integer(i) => Ok(Self::Integer(i)),
            LuaValue::Number(f) => {
                let f = NotNan::new(f).map_err(|_| mlua::Error::FromLuaConversionError {
                    from: value.type_name(),
                    to: "Value",
                    message: Some("NaN not supported".to_string()),
                })?;
                Ok(Self::Float(f))
            }
            LuaValue::Boolean(b) => Ok(Self::Boolean(b)),
            LuaValue::Table(t) => {
                if t.len()? > 0 {
                    <_>::from_lua(LuaValue::Table(t), lua).map(Self::Array)
                } else if table_is_timestamp(&t)? {
                    table_to_timestamp(t).map(Self::Timestamp)
                } else {
                    <_>::from_lua(LuaValue::Table(t), lua).map(Self::Object)
                }
            }
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: "Value",
                message: Some("Unsupported Lua type".to_string()),
            }),
        }
    }
}

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use mlua::prelude::*;

/// Convert a `DateTime<Utc>` to a `LuaTable`.
///
/// # Errors
///
/// This function will fail insertion into the table fails.
pub fn timestamp_to_table(lua: &Lua, ts: DateTime<Utc>) -> LuaResult<LuaTable<'_>> {
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
pub fn table_is_timestamp(t: &LuaTable<'_>) -> LuaResult<bool> {
    for &key in &["year", "month", "day", "hour", "min", "sec"] {
        if !t.contains_key(key)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Convert a `LuaTable` to a `DateTime<Utc>`.
///
/// # Errors
///
/// This function will fail if the table is malformed.
#[allow(clippy::needless_pass_by_value)] // constrained by mlua types
pub fn table_to_timestamp(t: LuaTable<'_>) -> LuaResult<DateTime<Utc>> {
    let year = t.raw_get("year")?;
    let month = t.raw_get("month")?;
    let day = t.raw_get("day")?;
    let hour = t.raw_get("hour")?;
    let min = t.raw_get("min")?;
    let sec = t.raw_get("sec")?;
    let nano = t.raw_get::<_, Option<u32>>("nanosec")?.unwrap_or(0);
    Ok(Utc
        .ymd(year, month, day)
        .and_hms_nano_opt(hour, min, sec, nano)
        .expect("invalid timestamp"))
}

#[cfg(test)]
mod test {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn from_lua() {
        let pairs = vec![
            (
                "'\u{237a}\u{3b2}\u{3b3}'",
                Value::Bytes("\u{237a}\u{3b2}\u{3b3}".into()),
            ),
            ("123", Value::Integer(123)),
            ("4.333", Value::from(4.333)),
            ("true", Value::Boolean(true)),
            (
                "{ x = 1, y = '2', nested = { other = 5.678 } }",
                Value::Object(
                    vec![
                        ("x".into(), 1_i64.into()),
                        ("y".into(), "2".into()),
                        (
                            "nested".into(),
                            Value::Object(
                                vec![("other".into(), 5.678.into())].into_iter().collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            (
                "{1, '2', 0.57721566}",
                Value::Array(vec![1_i64.into(), "2".into(), 0.577_215_66.into()]),
            ),
            (
                "os.date('!*t', 1584297428)",
                Value::Timestamp(
                    Utc.ymd(2020, 3, 15)
                        .and_hms_opt(18, 37, 8)
                        .expect("invalid timestamp"),
                ),
            ),
            (
                "{year=2020, month=3, day=15, hour=18, min=37, sec=8}",
                Value::Timestamp(
                    Utc.ymd(2020, 3, 15)
                        .and_hms_opt(18, 37, 8)
                        .expect("invalid timestamp"),
                ),
            ),
            (
                "{year=2020, month=3, day=15, hour=18, min=37, sec=8, nanosec=666666666}",
                Value::Timestamp(
                    Utc.ymd(2020, 3, 15)
                        .and_hms_nano_opt(18, 37, 8, 666_666_666)
                        .expect("invalid timestamp"),
                ),
            ),
        ];

        let lua = Lua::new();
        for (expression, expected) in pairs {
            let value: Value = lua.load(expression).eval().unwrap();
            assert_eq!(value, expected, "expression: {expression:?}");
        }
    }

    #[test]
    // Long test is long.
    #[allow(clippy::too_many_lines)]
    fn to_lua() {
        let pairs = vec![
            (
                Value::Bytes("\u{237a}\u{3b2}\u{3b3}".into()),
                r#"
                function (value)
                    return value == '\u{237a}\u{3b2}\u{3b3}'
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
                Value::from(4.333),
                r#"
                function (value)
                    return value == 4.333
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
                Value::Object(
                    vec![
                        ("x".into(), 1_i64.into()),
                        ("y".into(), "2".into()),
                        (
                            "nested".into(),
                            Value::Object(
                                vec![("other".into(), 5.111.into())].into_iter().collect(),
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
                        value.nested.other == 5.111
                end
                "#,
            ),
            (
                Value::Array(vec![1_i64.into(), "2".into(), 0.577_215_66.into()]),
                r#"
                function (value)
                    return value[1] == 1 and
                        value[2] == '2' and
                        value[3] == 0.57721566
                end
                "#,
            ),
            (
                Value::Timestamp(
                    Utc.ymd(2020, 3, 15)
                        .and_hms_nano_opt(18, 37, 8, 666_666_666)
                        .expect("invalid timestamp"),
                ),
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

        let lua = Lua::new();
        for (value, test_src) in pairs {
            let test_fn: LuaFunction<'_> = lua
                .load(test_src)
                .eval()
                .unwrap_or_else(|_| panic!("Failed to load {test_src} for value {value:?}"));
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
    }
}
