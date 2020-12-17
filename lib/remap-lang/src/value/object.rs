use crate::{Object, Path, Value};

impl Object for Value {
    fn insert(&mut self, path: &Path, value: Value) -> Result<(), String> {
        self.insert_by_path(path, value);
        Ok(())
    }

    fn get(&self, path: &Path) -> Result<Option<Value>, String> {
        Ok(self.get_by_path(path).cloned())
    }

    fn paths(&self) -> Result<Vec<Path>, String> {
        self.paths().map_err(|err| err.to_string())
    }

    fn remove(&mut self, path: &Path, compact: bool) -> Result<(), String> {
        self.remove_by_path(path, compact);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{map, value, Field::*, Segment::*};
    use std::str::FromStr;

    #[test]
    fn object_get() {
        let cases = vec![
            (value!(true), vec![], Ok(Some(value!(true)))),
            (
                value!(true),
                vec![Field(Regular("foo".to_string()))],
                Ok(None),
            ),
            (value!({}), vec![], Ok(Some(value!({})))),
            (value!({foo: "bar"}), vec![], Ok(Some(value!({foo: "bar"})))),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned()))],
                Ok(Some(value!("bar"))),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("bar".to_owned()))],
                Ok(None),
            ),
            (value!([1, 2, 3, 4, 5]), vec![Index(1)], Ok(Some(value!(2)))),
            (
                value!({foo: [{bar: true}]}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(0),
                    Field(Regular("bar".to_owned())),
                ],
                Ok(Some(value!(true))),
            ),
            (
                value!({foo: {"bar baz": {baz: 2}}}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Coalesce(vec![
                        Regular("qux".to_owned()),
                        Quoted("bar baz".to_owned()),
                    ]),
                    Field(Regular("baz".to_owned())),
                ],
                Ok(Some(value!(2))),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: Value = value;
            let path = Path::new_unchecked(segments);

            assert_eq!(value.get(&path), expect)
        }
    }

    #[test]
    fn object_insert() {
        let cases = vec![
            (
                map!["foo": "bar"],
                vec![],
                map!["baz": "qux"].into(),
                map!["baz": "qux"],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![Field(Regular("baz".to_owned()))],
                true.into(),
                map!["foo": "bar", "baz": true],
                Ok(()),
            ),
            (
                map!["foo": vec![map!["bar": "baz"]]],
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(0),
                    Field(Regular("baz".to_owned())),
                ],
                true.into(),
                map!["foo": Value::Array(vec![map!["bar": "baz", "baz": true].into()])],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![Field(Regular("foo".to_owned()))],
                "baz".into(),
                map!["foo": "baz"],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(2),
                    Field(Quoted("bar baz".to_owned())),
                    Field(Regular("a".to_owned())),
                    Field(Regular("b".to_owned())),
                ],
                true.into(),
                map![
                    "foo":
                        vec![
                            Value::Null,
                            Value::Null,
                            map!["bar baz": map!["a": map!["b": true]],].into()
                        ]
                ],
                Ok(()),
            ),
            (
                map!["foo": vec![0, 1, 2]],
                vec![Field(Regular("foo".to_owned())), Index(5)],
                "baz".into(),
                map![
                    "foo":
                        vec![
                            0.into(),
                            1.into(),
                            2.into(),
                            Value::Null,
                            Value::Null,
                            Value::from("baz"),
                        ]
                ],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![])],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into()])],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into(), 1.into()])],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                map!["foo": Value::Array(vec!["baz".into(), 1.into()])],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into(), 1.into()])],
                vec![Field(Regular("foo".to_owned())), Index(1)],
                "baz".into(),
                map!["foo": Value::Array(vec![0.into(), "baz".into()])],
                Ok(()),
            ),
        ];

        for (object, segments, value, expect, result) in cases {
            let mut object: Value = object.into();
            let expect: Value = expect.into();
            let value: Value = value;
            let path = Path::new_unchecked(segments);

            assert_eq!(Object::insert(&mut object, &path, value.clone()), result);
            assert_eq!(object, expect);
            assert_eq!(Object::get(&object, &path), Ok(Some(value)));
        }
    }

    #[test]
    fn object_remove() {
        let cases = vec![
            (
                map!["foo": "bar"].into(),
                vec![Field(Regular("foo".to_owned()))],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar"].into(),
                vec![Coalesce(vec![
                    Quoted("foo bar".to_owned()),
                    Regular("foo".to_owned()),
                ])],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar", "baz": "qux"].into(),
                vec![],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar", "baz": "qux"].into(),
                vec![],
                true,
                Some(map![].into()),
            ),
            (
                map!["foo": vec![0]].into(),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                false,
                Some(map!["foo": Value::Array(vec![])].into()),
            ),
            (
                map!["foo": vec![0]].into(),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                true,
                Some(map![].into()),
            ),
            (
                map!["foo": map!["bar baz": vec![0]], "bar": "baz"].into(),
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                false,
                Some(map!["foo": map!["bar baz": Value::Array(vec![])], "bar": "baz"].into()),
            ),
            (
                map!["foo": map!["bar baz": vec![0]], "bar": "baz"].into(),
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                true,
                Some(map!["bar": "baz"].into()),
            ),
        ];

        for (object, segments, compact, expect) in cases {
            let mut object: Value = object;
            let path = Path::new_unchecked(segments);

            assert_eq!(Object::remove(&mut object, &path, compact), Ok(()));
            assert_eq!(Object::get(&object, &Path::root()), Ok(expect))
        }
    }

    #[test]
    fn object_paths() {
        let cases = vec![
            (map![], Ok(vec![". "])),
            (
                map!["foo bar baz": "bar"],
                Ok(vec![". ", r#"."foo bar baz""#]),
            ),
            (
                map!["foo": "bar", "baz": "qux"],
                Ok(vec![". ", ".baz", ".foo"]),
            ),
            (
                map!["foo": map!["bar": "baz"]],
                Ok(vec![". ", ".foo", ".foo.bar"]),
            ),
            (
                map!["a": vec![0, 1]],
                Ok(vec![". ", ".a", ".a[0]", ".a[1]"]),
            ),
            (
                map!["a": map!["b": "c"], "d": 12, "e": vec![
                    map!["f": 1],
                    map!["g": 2],
                    map!["h": 3],
                ]],
                Ok(vec![
                    ". ", ".a", ".a.b", ".d", ".e", ".e[0]", ".e[0].f", ".e[1]", ".e[1].g",
                    ".e[2]", ".e[2].h",
                ]),
            ),
            (
                map![
                    "a": vec![map![
                        "b": vec![map!["c": map!["d": map!["e": vec![vec![0, 1]]]]]]
                    ]]
                ],
                Ok(vec![
                    ". ",
                    ".a",
                    ".a[0]",
                    ".a[0].b",
                    ".a[0].b[0]",
                    ".a[0].b[0].c",
                    ".a[0].b[0].c.d",
                    ".a[0].b[0].c.d.e",
                    ".a[0].b[0].c.d.e[0]",
                    ".a[0].b[0].c.d.e[0][0]",
                    ".a[0].b[0].c.d.e[0][1]",
                ]),
            ),
        ];

        for (object, expect) in cases {
            let object: Value = object.into();

            assert_eq!(
                Object::paths(&object),
                expect.map(|vec| vec.iter().map(|s| Path::from_str(s).unwrap()).collect())
            );
        }
    }
}
