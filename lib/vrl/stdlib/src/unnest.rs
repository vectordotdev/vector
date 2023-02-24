use ::value::Value;
use lookup_lib::{OwnedTargetPath, OwnedValuePath};
use vrl::prelude::*;

fn unnest(path: &expression::Query, ctx: &mut Context) -> Resolved {
    let lookup_buf = path.path();

    match path.target() {
        expression::Target::External(prefix) => {
            let root = ctx
                .target()
                .target_get(&OwnedTargetPath::root(*prefix))
                .expect("must never fail")
                .expect("always a value");
            unnest_root(root, lookup_buf)
        }
        expression::Target::Internal(v) => {
            let value = ctx.state().variable(v.ident()).unwrap_or(&Value::Null);
            let root = value.get(&OwnedValuePath::root()).expect("always a value");
            unnest_root(root, lookup_buf)
        }
        expression::Target::Container(expr) => {
            let value = expr.resolve(ctx)?;
            let root = value.get(&OwnedValuePath::root()).expect("always a value");
            unnest_root(root, lookup_buf)
        }
        expression::Target::FunctionCall(expr) => {
            let value = expr.resolve(ctx)?;
            let root = value.get(&OwnedValuePath::root()).expect("always a value");
            unnest_root(root, lookup_buf)
        }
    }
}

fn unnest_root(root: &Value, path: &OwnedValuePath) -> Resolved {
    let mut trimmed = root.clone();
    let values = trimmed
        .remove(path, true)
        .ok_or(value::Error::Expected {
            got: Kind::null(),
            expected: Kind::array(Collection::any()),
        })?
        .try_array()?;

    let events = values
        .into_iter()
        .map(|value| {
            let mut event = trimmed.clone();
            event.insert(path, value);
            event
        })
        .collect::<Vec<_>>();

    Ok(Value::Array(events))
}

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
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let path = arguments.required_query("path")?;
        Ok(UnnestFn { path }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct UnnestFn {
    path: expression::Query,
}

impl UnnestFn {
    #[cfg(test)]
    fn new(path: &str) -> Self {
        use lookup_lib::{lookup_v2::parse_value_path, PathPrefix};

        Self {
            path: expression::Query::new(
                expression::Target::External(PathPrefix::Event),
                parse_value_path(path).unwrap(),
            ),
        }
    }
}

impl FunctionExpression for UnnestFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        unnest(&self.path, ctx)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        use expression::Target;

        match self.path.target() {
            Target::External(prefix) => invert_array_at_path(
                &TypeDef::from(state.external.kind(*prefix)),
                self.path.path(),
            ),
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
/// `{ "a" => { "b" => [ { "c" => 2 }, { "c" => 3 } ] } }`
///
/// Is converted to a typedef for this array:
/// `[ { "a" => { "b" => { "c" => 2 } } },
///    { "a" => { "b" => { "c" => 3 } } },
///  ]`
///
pub(crate) fn invert_array_at_path(typedef: &TypeDef, path: &OwnedValuePath) -> TypeDef {
    let kind = typedef.kind().at_path(path);

    let mut array = if let Some(array) = kind.into_array() {
        array
    } else {
        // Guaranteed fallible.
        // This can't actually be set to "fallible", or it will cause problems due to
        // https://github.com/vectordotdev/vector/issues/13527
        return TypeDef::never();
    };

    array.known_mut().values_mut().for_each(|kind| {
        let mut tdkind = typedef.kind().clone();
        tdkind.insert(path, kind.clone());

        *kind = tdkind.clone();
    });

    let unknown = array.unknown_kind();
    if unknown.contains_any_defined() {
        let mut tdkind = typedef.kind().clone();
        tdkind.insert(path, unknown.without_undefined());
        array.set_unknown(tdkind);
    }

    TypeDef::array(array).infallible()
}

#[cfg(test)]
mod tests {
    use lookup_lib::lookup_v2::parse_value_path;
    use vector_common::{btreemap, TimeZone};
    use vrl::state::TypeState;

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
            // Indexed specific
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array {
                        0 => type_def! { object {
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
                            0 => type_def! { object {
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
            // Coalesce with known path first.
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
                            "shnoog" => {
                                type_def! { object {
                                    "noog" => type_def! { bytes },
                                    "nork" => type_def! { bytes },
                                } }
                            },
                        } },
                    } },
                ] },
            },
            // Coalesce with known path second
            TestCase {
                old: type_def! { object {
                    unknown => type_def! { bytes },
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
                path: ".(nork | nonk).shnoog",
                new: type_def! { array [
                    type_def! { object {
                        unknown => type_def! { bytes },
                        "nonk" => type_def! { object {
                            "shnoog" => type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } }.union(type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        ] }),
                        } },
                    } },
                ] },
            },
            // Nonexistent, the types we know are moved into the returned array.
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { bytes },
                } },
                path: ".norg",
                // guaranteed to fail at runtime
                new: TypeDef::never(),
            },
        ];

        for case in cases {
            let path = parse_value_path(case.path).unwrap();
            let new = invert_array_at_path(&case.old, &path);
            assert_eq!(case.new, new, "{path}");
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
                Err("expected array, got null".to_owned()),
                UnnestFn::new("unknown"),
                // guaranteed to always fail
                TypeDef::never(),
            ),
            (
                value!({"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}),
                Err("expected array, got string".to_owned()),
                UnnestFn::new("hostname"),
                // guaranteed to always fail
                TypeDef::never(),
            ),
        ];

        let local = state::LocalEnv::default();
        let external = state::ExternalEnv::new_with_kind(
            Kind::object(btreemap! {
                "hostname" => Kind::bytes(),
                "events" => Kind::array(Collection::from_unknown(Kind::object(btreemap! {
                    Field::from("message") => Kind::bytes(),
                })),
            )}),
            Kind::object(Collection::empty()),
        );
        let state = TypeState { local, external };

        let tz = TimeZone::default();
        for (object, expected, func, expected_typedef) in cases {
            let mut object = object.clone();
            let mut runtime_state = vrl::state::Runtime::default();
            let mut ctx = Context::new(&mut object, &mut runtime_state, &tz);

            let got_typedef = func.type_def(&state);

            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, expected);
            assert_eq!(got_typedef, expected_typedef);
        }
    }
}
