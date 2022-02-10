use lookup_lib::LookupBuf;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Unnest;

impl Function for Unnest {
    fn identifier(&self) -> &'static str {
        "unnest"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "path",
            kind: kind::ARRAY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "external target",
                source: indoc! {r#"
                    . = {"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}
                    . = unnest(.events)
                "#},
                result: Ok(
                    r#"[{"hostname": "localhost", "events": {"message": "hello"}}, {"hostname": "localhost", "events": {"message": "world"}}]"#,
                ),
            },
            Example {
                title: "variable target",
                source: indoc! {r#"
                    foo = {"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}
                    foo = unnest(foo.events)
                "#},
                result: Ok(
                    r#"[{"hostname": "localhost", "events": {"message": "hello"}}, {"hostname": "localhost", "events": {"message": "world"}}]"#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let path = arguments.required_query("path")?;

        Ok(Box::new(UnnestFn { path }))
    }
}

#[derive(Debug, Clone)]
struct UnnestFn {
    path: expression::Query,
}

impl UnnestFn {
    #[cfg(test)]
    fn new(path: &str) -> Self {
        use std::str::FromStr;

        Self {
            path: expression::Query::new(
                expression::Target::External,
                FromStr::from_str(path).unwrap(),
            ),
        }
    }
}

impl Expression for UnnestFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let path = self.path.path();

        let value: Value;
        let target: Box<&dyn Target> = match self.path.target() {
            expression::Target::External => Box::new(ctx.target()) as Box<_>,
            expression::Target::Internal(v) => {
                let v = ctx.state().variable(v.ident()).unwrap_or(&Value::Null);
                Box::new(v as &dyn Target) as Box<_>
            }
            expression::Target::Container(expr) => {
                value = expr.resolve(ctx)?;
                Box::new(&value as &dyn Target) as Box<&dyn Target>
            }
            expression::Target::FunctionCall(expr) => {
                value = expr.resolve(ctx)?;
                Box::new(&value as &dyn Target) as Box<&dyn Target>
            }
        };

        let root = target.get(&LookupBuf::root())?.unwrap_or(Value::Null);

        let values = root
            .get_by_path(path)
            .cloned()
            .ok_or(value::Error::Expected {
                got: Kind::Null,
                expected: Kind::Array,
            })?
            .try_array()?;

        let events = values
            .into_iter()
            .map(|value| {
                let mut event = root.clone();
                event.insert_by_path(path, value);
                event
            })
            .collect::<Vec<_>>();

        Ok(Value::Array(events))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use expression::Target;

        match self.path.target() {
            Target::External => match state.target_type_def() {
                Some(root_type_def) => invert_array_at_path(root_type_def, self.path.path()),
                None => self.path.type_def(state).restrict_array().add_null(),
            },
            Target::Internal(v) => invert_array_at_path(&v.type_def(state), self.path.path()),
            Target::FunctionCall(f) => invert_array_at_path(&f.type_def(state), self.path.path()),
            Target::Container(c) => invert_array_at_path(&c.type_def(state), self.path.path()),
        }
    }
}

/// Assuming path points at an Array, this will take the typedefs for that array,
/// And will remove it returning a set of it's elements.
///
/// For example the typedef for this object:
/// `{ "nonk" => { "shnoog" => [ { "noog" => 2 }, { "noog" => 3 } ] } }`
///
/// Is converted to a typedef for this array:
/// `[ { "nonk" => { "shnoog" => { "noog" => 2 } } },
///    { "nonk" => { "shnoog" => { "noog" => 3 } } },
///  ]`
///
pub fn invert_array_at_path(typedef: &TypeDef, path: &LookupBuf) -> TypeDef {
    typedef
        .at_path(path.clone())
        .restrict_array()
        .map_array(|kind| typedef.update_path(path, kind).kind)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use type_def::KindInfo;
    use vector_common::{btreemap, TimeZone};
    use vrl::Index;

    use super::*;

    #[test]
    fn type_def() {
        struct TestCase {
            old: TypeDef,
            path: &'static str,
            new: TypeDef,
        }

        let cases = vec![
            // Simple case
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array [
                        type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    ] },
                } },
                path: ".nonk",
                new: type_def! { array [
                    type_def! { object {
                        "nonk" => type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    } },
                ] },
            },
            // Provided example
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
                path: "nonk.shnoog",
                new: type_def! { array [
                    type_def! { object {
                        "nonk" => type_def! { object {
                            "shnoog" => type_def! { object {
                                "noog" => type_def! { bytes },
                            } },
                        } },
                    } },
                ] },
            },
            // Same field in different branches
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                            } },
                        ] },
                    } },
                    "nink" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
                path: "nonk.shnoog",
                new: type_def! { array [
                    type_def! { object {
                        "nonk" => type_def! { object {
                            "shnoog" => type_def! { object {
                                "noog" => type_def! { bytes },
                            } },
                        } },
                        "nink" => type_def! { object {
                            "shnoog" => type_def! { array [
                                type_def! { object {
                                    "noog" => type_def! { bytes },
                                } },
                            ] },
                        } },
                    } },
                ] },
            },
            // Indexed any
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array [
                        type_def! { object {
                            "noog" => type_def! { array [
                                type_def! { bytes },
                            ] },
                            "nork" => type_def! { bytes },
                        } },
                    ] },
                } },
                path: ".nonk[0].noog",
                new: type_def! { array [
                    type_def! { object {
                        "nonk" => type_def! { array {
                            (Index::Any) => type_def! { object {
                                "noog" => type_def! { array [
                                    type_def! { bytes },
                                ] },
                                "nork" => type_def! { bytes },
                            } },
                            // The index is added on top of the Any entry.
                            (Index::Index(0)) => type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        } },
                    } },
                ] },
            },
            // Indexed specific
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array {
                        Index::Index(0) => type_def! { object {
                            "noog" => type_def! { array [
                                type_def! { bytes },
                            ] },
                            "nork" => type_def! { bytes },
                        } },
                    } },
                } },
                path: ".nonk[0].noog",
                new: type_def! { array [
                    type_def! { object {
                        "nonk" => type_def! { array {
                            // The index is added on top of the Any entry.
                            (Index::Index(0)) => type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        } },
                    } },
                ] },
            },
            // More nested
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
                path: ".nonk.shnoog",
                new: type_def! { array [
                    type_def! { object {
                        "nonk" => type_def! { object {
                            "shnoog" => type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        } },
                    } },
                ] },
            },
            // Coalesce
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
                path: ".(nonk | nork).shnoog",
                new: type_def! { array [
                    type_def! { object {
                        "nonk" => type_def! { object {
                            "shnoog" => type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        } }.add_null(),
                    } },
                ] },
            },
            // Non existent, the types we know are moved into the returned array.
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { bytes },
                } },
                path: ".norg",
                new: type_def! { array [
                    type_def! { object {
                        "nonk" => type_def! { bytes },
                    } },
                ] },
            },
        ];

        for case in cases {
            let path = LookupBuf::from_str(case.path).unwrap();
            let new = invert_array_at_path(&case.old, &path);
            assert_eq!(case.new, new, "{}", path);
        }
    }

    #[test]
    fn unnest() {
        let cases = vec![
            (
                value!({"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}),
                Ok(
                    value!([{"hostname": "localhost", "events": {"message": "hello"}}, {"hostname": "localhost", "events": {"message": "world"}}]),
                ),
                UnnestFn::new("events"),
                type_def! { array [
                    type_def! { object {
                        "hostname" => type_def! { bytes },
                        "events" => type_def! { object {
                            "message" => type_def! { bytes },
                        } },
                    } },
                ] },
            ),
            (
                value!({"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}),
                Err(r#"expected "array", got "null""#.to_owned()),
                UnnestFn::new("unknown"),
                type_def! { array [
                    type_def! { object {
                        "hostname" => type_def! { bytes },
                        "events" => type_def! { array [
                            type_def! { object {
                                "message" => type_def! { bytes },
                            } },
                        ] },
                    } },
                ] },
            ),
            (
                value!({"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}),
                Err(r#"expected "array", got "string""#.to_owned()),
                UnnestFn::new("hostname"),
                // The typedef in this case is not particularly important as we will have a compile
                // error before we get to this point.
                TypeDef {
                    fallible: false,
                    kind: KindInfo::Known(BTreeSet::new()),
                },
            ),
        ];

        let compiler = state::Compiler::new_with_type_def(
            TypeDef::new().object::<&'static str, TypeDef>(btreemap! {
                "hostname" => TypeDef::new().bytes(),
                "events" => TypeDef::new().array_mapped::<(), TypeDef>(btreemap! {
                        () => TypeDef::new().object::<&'static str, TypeDef>(btreemap! {
                            "message" => TypeDef::new().bytes()
                        })
                }),
            }),
        );

        let tz = TimeZone::default();
        for (object, expected, func, expected_typedef) in cases {
            let mut object = object.clone();
            let mut runtime_state = vrl::state::Runtime::default();
            let mut ctx = Context::new(&mut object, &mut runtime_state, &tz);

            let typedef = func.type_def(&compiler);

            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, expected);
            assert_eq!(typedef, expected_typedef);
        }
    }
}
