use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ForEach;

impl Function for ForEach {
    fn identifier(&self) -> &'static str {
        "for_each"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::OBJECT | kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "recursive",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "iterate object",
                source: r#"count = 0; for_each({ "a": 1, "b": 2 }) -> |_key, value| { count = count + int!(value) }; count"#,
                result: Ok("3"),
            },
            Example {
                title: "recursively iterate object",
                source: r#"count = 0; for_each({ "a": 1, "b": { "c": 2, "d": 3 } }, recursive: true) -> |_, value| { count = count + (int(value) ?? 0) }; count"#,
                result: Ok("6"),
            },
            Example {
                title: "iterate array",
                source: r#"count = 0; for_each([1,2,3]) -> |index, value| { count = count + index + int!(value) }; count"#,
                result: Ok("9"),
            },
            Example {
                title: "recursively iterate array",
                source: r#"count = 0; for_each([1,2,[3,4]], recursive: true) -> |index, value| { count = count + index + (int(value) ?? 0) }; count"#,
                result: Ok("14"),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let recursive = arguments.optional("recursive");
        let closure = arguments.required_closure()?;

        Ok(Box::new(ForEachFn {
            value,
            closure,
            recursive,
        }))
    }

    fn closure(&self) -> Option<closure::Definition> {
        let object = closure::Input {
            parameter_keyword: "value",
            kind: Kind::object(Collection::any()),
            variables: vec![
                closure::Variable {
                    kind: Kind::bytes(),
                },
                closure::Variable { kind: Kind::any() },
            ],
            output: closure::Output::Any,
            example: Example {
                title: "iterate object",
                source: r#"for_each({ "one" : 1, "two": 2 }) -> |key, value| { .foo = to_int!(.foo) + int!(value) }"#,
                result: Ok("null"),
            },
        };

        let array = closure::Input {
            parameter_keyword: "value",
            kind: Kind::array(Collection::any()),
            variables: vec![
                closure::Variable {
                    kind: Kind::integer(),
                },
                closure::Variable { kind: Kind::any() },
            ],
            output: closure::Output::Any,
            example: Example {
                title: "iterate array",
                source: r#"for_each([1, 2]) -> |index, value| { .foo = to_int!(.foo) + index + int!(value) }"#,
                result: Ok("null"),
            },
        };

        Some(closure::Definition {
            inputs: vec![object, array],
        })
    }

    fn call_by_vm(&self, _ctx: &mut Context, _args: &mut VmArgumentList) -> Result<Value> {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct ForEachFn {
    value: Box<dyn Expression>,
    recursive: Option<Box<dyn Expression>>,
    closure: FunctionClosure,
}

impl Expression for ForEachFn {
    fn resolve(&self, ctx: &mut Context) -> Result<Value> {
        let recursive = match &self.recursive {
            None => false,
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
        };

        match self.value.resolve(ctx)? {
            Value::Object(object) => {
                let mut iter = ObjectIterator::new(object, recursive);

                while let Some((key, value)) = iter.iter_mut() {
                    // We do not care about the result of the closure, as we
                    // don't need to mutate any state, and the function itself
                    // always returns `null`.
                    let _ = self
                        .closure
                        .key_value(key.clone(), value.clone())
                        .resolve(ctx)?;
                }

                Ok(Value::Null)
            }
            Value::Array(array) => {
                let mut iter = ArrayIterator::new(array, recursive);

                while let Some((index, value)) = iter.iter_mut() {
                    // We do not care about the result of the closure, as we
                    // don't need to mutate any state, and the function itself
                    // always returns `null`.
                    let _ = self
                        .closure
                        .index_value(index, value.clone())
                        .resolve(ctx)?;
                }

                Ok(Value::Null)
            }
            _ => unreachable!("expected object or array"),
        }
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        // FIXME(Jean): fallibility of the function needs to take into account
        // the fallibility of the closure block. But we should check this at the
        // compiler-level, so that function implementors don't need to care
        // about that.
        TypeDef::null().infallible()
    }
}

// --------------------------------------

#[derive(Debug)]
struct ObjectIterator {
    entries: Vec<ObjectEntry>,
    index: usize,
}

#[derive(Debug)]
struct ObjectEntry {
    key: String,
    value: ObjectValue,
}

#[derive(Debug)]
enum ObjectValue {
    Value(Value),
    Object {
        value: Value,
        iter: Box<ObjectIterator>,
    },
}

impl ObjectValue {
    fn into_value(self) -> Value {
        match self {
            Self::Value(value) => value,
            Self::Object { value, .. } => value,
        }
    }
}

impl ObjectIterator {
    fn new(object: BTreeMap<String, Value>, recursive: bool) -> Self {
        let entries = object
            .into_iter()
            .map(|(key, value)| ObjectEntry {
                key,
                value: match value {
                    Value::Object(object) if recursive => ObjectValue::Object {
                        value: object.clone().into(),
                        iter: Box::new(ObjectIterator::new(object, true)),
                    },
                    value => ObjectValue::Value(value),
                },
            })
            .collect::<Vec<_>>();

        Self { entries, index: 0 }
    }

    fn iter_mut(&mut self) -> Option<(&mut String, &mut Value)> {
        let entry = self.entries.get_mut(self.index)?;
        let value = match &mut entry.value {
            ObjectValue::Value(value) => value,
            ObjectValue::Object { value, iter } => {
                while let Some(item) = iter.iter_mut() {
                    return Some(item);
                }

                value
            }
        };

        self.index += 1;
        Some((&mut entry.key, value))
    }
}

impl From<ObjectIterator> for Value {
    fn from(iter: ObjectIterator) -> Self {
        iter.entries
            .into_iter()
            .map(|entry| (entry.key, entry.value.into_value()))
            .collect::<BTreeMap<_, _>>()
            .into()
    }
}

// --------------------------------------

#[derive(Debug)]
struct ArrayIterator {
    entries: Vec<ArrayEntry>,
    index: usize,
}

#[derive(Debug)]
struct ArrayEntry {
    index: usize,
    value: ArrayValue,
}

#[derive(Debug)]
enum ArrayValue {
    Value(Value),
    Array {
        value: Value,
        iter: Box<ArrayIterator>,
    },
}

impl ArrayValue {
    fn into_value(self) -> Value {
        match self {
            Self::Value(value) => value,
            Self::Array { value, .. } => value,
        }
    }
}

impl ArrayIterator {
    fn new(array: Vec<Value>, recursive: bool) -> Self {
        let entries = array
            .into_iter()
            .enumerate()
            .map(|(index, value)| ArrayEntry {
                index,
                value: match value {
                    Value::Array(array) if recursive => ArrayValue::Array {
                        value: array.clone().into(),
                        iter: Box::new(ArrayIterator::new(array, true)),
                    },
                    value => ArrayValue::Value(value),
                },
            })
            .collect::<Vec<_>>();

        Self { entries, index: 0 }
    }

    fn iter_mut(&mut self) -> Option<(usize, &mut Value)> {
        let entry = self.entries.get_mut(self.index)?;
        let value = match &mut entry.value {
            ArrayValue::Value(value) => value,
            ArrayValue::Array { value, iter } => {
                while let Some(item) = iter.iter_mut() {
                    return Some(item);
                }

                value
            }
        };

        self.index += 1;
        Some((entry.index, value))
    }
}

impl From<ArrayIterator> for Value {
    fn from(iter: ArrayIterator) -> Self {
        iter.entries
            .into_iter()
            .map(|entry| (entry.value.into_value()))
            .collect::<Vec<_>>()
            .into()
    }
}
