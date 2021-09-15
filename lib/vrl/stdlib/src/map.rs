use std::collections::BTreeMap;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Map;

impl Function for Map {
    fn identifier(&self) -> &'static str {
        "map"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::OBJECT | kind::ARRAY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[/* TODO */]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let closure = arguments.required_closure()?;

        Ok(Box::new(MapFn { value, closure }))
    }

    fn closure(&self) -> Option<closure::Definition> {
        let object = closure::Input {
            parameter_keyword: "value",
            kind: Kind::Object,
            variables: vec![
                closure::Variable { kind: Kind::Bytes },
                closure::Variable { kind: Kind::all() },
            ],
            output: closure::Output::Array {
                elements: vec![Kind::Bytes, Kind::all()],
            },
        };

        let array = closure::Input {
            parameter_keyword: "value",
            kind: Kind::Array,
            variables: vec![
                closure::Variable {
                    kind: Kind::Integer,
                },
                closure::Variable { kind: Kind::all() },
            ],
            output: closure::Output::Any,
        };

        Some(closure::Definition {
            inputs: vec![object, array],
        })
    }
}

#[derive(Debug, Clone)]
struct MapFn {
    value: Box<dyn Expression>,
    closure: Closure,
}

impl Expression for MapFn {
    fn resolve(&self, ctx: &mut Context) -> Result<Value> {
        // - First focus on objects, ignore arrays
        // - A closure can resolve if it knows:
        //   - Closure variable signature/identifiers
        //   - Content of the object
        //   - Access to `ctx`
        //
        // - Resolving closure should be agnostic. It takes the values you want to assign to
        //   different closure variables, and it takes an `Fn` to apply to the data.

        let mut result: BTreeMap<String, Value> = BTreeMap::default();

        let value = self.value.resolve(ctx)?;

        let mut map = |_: &Context, output: Output| -> Result<()> {
            match output {
                Output::Object { key, value } => result.insert(key, value),
            };

            Ok(())
        };

        self.closure.resolve(ctx, value, &mut map)?;

        Ok(result.into())

        // let result = match self.value.resolve(ctx)? {
        //     Value::Object(object) => {
        //         let mut result = HashMap::default();

        //         for (key, value) in object.into_iter() {
        //             let ident = key.into();

        //             ctx.state_mut().insert_variable(ident, value);
        //             let v = self.closure.resolve(ctx)?.try_array()?;

        //             let v = closure.resolve_object(ctx, object)?;

        //             ctx.state_mut().remove_variable(&ident);

        //             result.insert(v[0], v[1]);
        //         }

        //         result.into()
        //     }
        //     Value::Array(array) => {
        //         let mut result = Vec::with_capacity(array.len());

        //         for (index, value) in array.into_iter().enumerate() {
        //             ctx.state_mut().insert_variable("index".into(), index);

        //             let v = run(index, value)?;
        //             result.push(v);
        //         }

        //         result.into()
        //     }
        //     _ => unreachable!("expected object or array"),
        // };
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::Object | Kind::Array)
            .restrict_array()
    }
}
