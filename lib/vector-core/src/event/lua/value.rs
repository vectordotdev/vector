use mlua::prelude::*;

use super::util::{table_is_timestamp, table_to_timestamp, timestamp_to_table};
use crate::event::Value;

//TODO: move these tests to lib/value

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
            ("4.333", Value::Float(4.333)),
            ("true", Value::Boolean(true)),
            (
                "{ x = 1, y = '2', nested = { other = 5.678 } }",
                Value::Map(
                    vec![
                        ("x".into(), 1.into()),
                        ("y".into(), "2".into()),
                        (
                            "nested".into(),
                            Value::Map(vec![("other".into(), 5.678.into())].into_iter().collect()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            (
                "{1, '2', 0.57721566}",
                Value::Array(vec![1.into(), "2".into(), 0.577_215_66.into()]),
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
                Value::Timestamp(Utc.ymd(2020, 3, 15).and_hms_nano(18, 37, 8, 666_666_666)),
            ),
        ];

        let lua = Lua::new();
        for (expression, expected) in pairs {
            let value: Value = lua.load(expression).eval().unwrap();
            assert_eq!(value, expected, "expression: {:?}", expression);
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
                Value::Float(4.333),
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
                Value::Map(
                    vec![
                        ("x".into(), 1.into()),
                        ("y".into(), "2".into()),
                        (
                            "nested".into(),
                            Value::Map(vec![("other".into(), 5.111.into())].into_iter().collect()),
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
                Value::Array(vec![1.into(), "2".into(), 0.577_215_66.into()]),
                r#"
                function (value)
                    return value[1] == 1 and
                        value[2] == '2' and
                        value[3] == 0.57721566
                end
                "#,
            ),
            (
                Value::Timestamp(Utc.ymd(2020, 3, 15).and_hms_nano(18, 37, 8, 666_666_666)),
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
            let test_fn: LuaFunction = lua
                .load(test_src)
                .eval()
                .unwrap_or_else(|_| panic!("Failed to load {} for value {:?}", test_src, value));
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
