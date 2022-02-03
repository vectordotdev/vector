use crate::{Target, Value};
use lookup::LookupBuf;

impl Target for Value {
    fn insert(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
        self.insert_by_path(path, value);
        Ok(())
    }

    fn get(&self, path: &LookupBuf) -> Result<Option<Value>, String> {
        Ok(self.get_by_path(path).cloned())
    }

    fn remove(&mut self, path: &LookupBuf, compact: bool) -> Result<Option<Value>, String> {
        let value = Target::get(self, path)?;
        self.remove_by_path(path, compact);

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)] // tests

    use super::*;
    use crate::value;
    use lookup::FieldBuf;
    use lookup::SegmentBuf;

    #[test]
    fn target_get() {
        let cases = vec![
            (value!(true), vec![], Ok(Some(value!(true)))),
            (value!(true), vec![SegmentBuf::from("foo")], Ok(None)),
            (value!({}), vec![], Ok(Some(value!({})))),
            (value!({foo: "bar"}), vec![], Ok(Some(value!({foo: "bar"})))),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                Ok(Some(value!("bar"))),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("bar")],
                Ok(None),
            ),
            (
                value!([1_i64, 2_i64, 3_i64, 4_i64, 5_i64]),
                vec![SegmentBuf::from(1)],
                Ok(Some(value!(2_i64))),
            ),
            (
                value!({foo: [{bar: true}]}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(0),
                    SegmentBuf::from("bar"),
                ],
                Ok(Some(value!(true))),
            ),
            (
                value!({foo: {"bar baz": {baz: 2_i64}}}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(vec![FieldBuf::from("qux"), FieldBuf::from(r#""bar baz""#)]),
                    SegmentBuf::from("baz"),
                ],
                Ok(Some(value!(2_i64))),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: Value = value;
            let path = LookupBuf::from_segments(segments);

            assert_eq!(Target::get(&value, &path), expect);
        }
    }

    #[test]
    fn target_insert() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                vec![],
                value!({baz: "qux"}),
                value!({baz: "qux"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("baz")],
                true.into(),
                value!({foo: "bar", baz: true}),
                Ok(()),
            ),
            (
                value!({foo: [{bar: "baz"}]}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(0),
                    SegmentBuf::from("baz"),
                ],
                true.into(),
                value!({foo: [{bar: "baz", baz: true}]}),
                Ok(()),
            ),
            (
                value!({foo: {bar: "baz"}}),
                vec![SegmentBuf::from("bar"), SegmentBuf::from("baz")],
                true.into(),
                value!({foo: {bar: "baz"}, bar: {baz: true}}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                "baz".into(),
                value!({foo: "baz"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(2),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from("a"),
                    SegmentBuf::from("b"),
                ],
                true.into(),
                value!({foo: [null, null, {"bar baz": {"a": {"b": true}}}]}),
                Ok(()),
            ),
            /*
            (
                value!({foo: [0, 1, 2]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(5)],
                "baz".into(),
                value!({foo: [0, 1, 2, null, null, "baz"]}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: []}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                value!({foo: ["baz", 1]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(1)],
                "baz".into(),
                value!({foo: [0, "baz"]}),
                Ok(()),
            ),
            */
        ];

        for (mut target, segments, value, expect, result) in cases {
            println!("Inserting at {:?}", segments);
            let path = LookupBuf::from_segments(segments);

            assert_eq!(Target::insert(&mut target, &path, value.clone()), result);
            assert_eq!(target, expect);
            assert_eq!(Target::get(&target, &path), Ok(Some(value)));
        }
    }

    #[test]
    fn target_remove() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("baz")],
                false,
                None,
                Some(value!({foo: "bar"})),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::from("foo")],
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar"}),
                vec![SegmentBuf::coalesce(vec![
                    FieldBuf::from(r#""foo bar""#),
                    FieldBuf::from("foo"),
                ])],
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar", baz: "qux"}),
                vec![],
                false,
                Some(value!({foo: "bar", baz: "qux"})),
                Some(value!({})),
            ),
            (
                value!({foo: "bar", baz: "qux"}),
                vec![],
                true,
                Some(value!({foo: "bar", baz: "qux"})),
                Some(value!({})),
            ),
            (
                value!({foo: [0_i64]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                false,
                Some(value!(0_i64)),
                Some(value!({foo: []})),
            ),
            (
                value!({foo: [0_i64]}),
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                true,
                Some(value!(0_i64)),
                Some(value!({})),
            ),
            (
                value!({foo: {"bar baz": [0_i64]}, bar: "baz"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from(0),
                ],
                false,
                Some(value!(0_i64)),
                Some(value!({foo: {"bar baz": []}, bar: "baz"})),
            ),
            (
                value!({foo: {"bar baz": [0_i64]}, bar: "baz"}),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from(0),
                ],
                true,
                Some(value!(0_i64)),
                Some(value!({bar: "baz"})),
            ),
        ];

        for (mut target, segments, compact, value, expect) in cases {
            let path = LookupBuf::from_segments(segments);

            assert_eq!(Target::remove(&mut target, &path, compact), Ok(value));
            assert_eq!(Target::get(&target, &LookupBuf::root()), Ok(expect));
        }
    }
}
