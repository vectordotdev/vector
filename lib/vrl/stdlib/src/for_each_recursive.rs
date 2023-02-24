use ::value::Value;
use std::collections::btree_map;
use vrl::prelude::*;

fn for_each_recursive<T>(value: Value, ctx: &mut Context, runner: closure::Runner<T>) -> Resolved
where
    T: Fn(&mut Context) -> Resolved,
{
    match value {
        Value::Object(map) => {
            let map_parent = MapParent::new(map.iter());
            for (keys, val) in map_parent {
                runner.run_keys_value(ctx, keys, val)?;
            }
            Ok(().into())
        }
        value => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::object(Collection::any()),
        }
        .into()),
    }
}

/// An iterator to go through the maps allowing the full path to the value to be provided.
struct MapParent<'a> {
    values: btree_map::Iter<'a, String, Value>,
    inner: Option<Box<MapParent<'a>>>,
    parent: Option<Vec<String>>,
}

impl<'a> MapParent<'a> {
    fn new(values: btree_map::Iter<'a, String, Value>) -> Self {
        Self {
            values,
            inner: None,
            parent: None,
        }
    }

    fn new_from_parent(parent: Vec<String>, values: btree_map::Iter<'a, String, Value>) -> Self {
        Self {
            values,
            inner: None,
            parent: Some(parent),
        }
    }

    fn new_key(&self, key: &str) -> Vec<String> {
        match &self.parent {
            None => vec![key.to_string()],
            Some(parent) => {
                let mut copy = parent.clone();
                copy.push(key.to_string());
                copy
            }
        }
    }
}

impl<'a> std::iter::Iterator for MapParent<'a> {
    type Item = (Vec<String>, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ref mut inner) = self.inner {
            let next = inner.next();
            match next {
                Some(_) => return next,
                None => self.inner = None,
            }
        }

        let next = self.values.next();
        match next {
            Some((key, Value::Object(value))) => {
                self.inner = Some(Box::new(MapParent::new_from_parent(
                    self.new_key(key),
                    value.iter(),
                )));
                self.next()
            }
            Some((key, value)) => Some((self.new_key(key), value)),
            None => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ForEachRecursive;

impl Function for ForEachRecursive {
    fn identifier(&self) -> &'static str {
        "for_each_recursive"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::OBJECT,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "iterate object key values",
            source: r#".example = {}; .example.child = "123"; for_each_recursive(.) -> |keys, value| { if join!(keys, ".") == "example.child" { . = remove!(., keys) }}; ."#,
            result: Ok(r#"{"example":{}}"#),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let closure = arguments.required_closure()?;

        Ok(ForEachRecursiveFn { value, closure }.as_expr())
    }

    fn closure(&self) -> Option<closure::Definition> {
        use closure::{Definition, Input, Output, Variable, VariableKind};

        Some(Definition {
            inputs: vec![Input {
                parameter_keyword: "value",
                kind: Kind::object(Collection::any()).or_array(Collection::any()),
                variables: vec![
                    Variable {
                        kind: VariableKind::Exact(Kind::array(Collection::any())),
                    },
                    Variable {
                        kind: VariableKind::Exact(Kind::any()),
                    },
                ],
                output: Output::Kind(Kind::any()),
                example: Example {
                    title: "iterate object keys/values",
                    source: r#".example = {}; .example.child = "123"; for_each_recursive(.) -> |keys, value| { if join!(keys, ".") == "example.child" { . = remove!(., keys) }}; ."#,
                    result: Ok(r#"{"example":{}}"#),
                },
            }],
            is_iterator: true,
        })
    }
}

#[derive(Debug, Clone)]
struct ForEachRecursiveFn {
    value: Box<dyn Expression>,
    closure: FunctionClosure,
}

impl FunctionExpression for ForEachRecursiveFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let FunctionClosure {
            variables,
            block,
            block_type_def: _,
        } = &self.closure;
        let runner = closure::Runner::new(variables, |ctx| block.resolve(ctx));

        for_each_recursive(value, ctx, runner)
    }

    fn type_def(&self, ctx: &state::TypeState) -> TypeDef {
        let mut value = self.value.type_def(ctx);
        let closure = self.closure.block_type_def.kind().clone();

        recursive_type_def(&mut value, closure, true);
        value
    }
}

fn recursive_type_def(from: &mut Kind, to: Kind, root: bool) {
    if let Some(object) = from.as_object_mut() {
        for v in object.known_mut().values_mut() {
            recursive_type_def(v, to.clone(), false)
        }
    }

    if let Some(array) = from.as_array_mut() {
        for v in array.known_mut().values_mut() {
            recursive_type_def(v, to.clone(), false)
        }
    }

    if !root {
        *from = to;
    }
}
