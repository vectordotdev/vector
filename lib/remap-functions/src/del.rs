use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Del;

impl Function for Del {
    fn identifier(&self) -> &'static str {
        "del"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "path",
            accepts: |_| true,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let path = arguments.required_path("path")?;

        Ok(Box::new(DelFn { path }))
    }
}

#[derive(Debug, Clone)]
pub struct DelFn {
    path: Path,
}

impl DelFn {
    #[cfg(test)]
    fn new(path: Path) -> Self {
        Self { path }
    }
}

impl Expression for DelFn {
    fn execute(&self, _: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        // TODO: we're silencing the result of the `remove` call here, to make
        // this function infallible.
        //
        // This isn't correct though, since, while deleting Vector log fields is
        // infallible, deleting metric fields is not.
        //
        // For example, if you try to delete `.name` in a metric event, the call
        // returns an error, since this is an immutable field.
        //
        // After some debating, we've decided to _silently ignore_ deletions of
        // immutable fields for now, but we'll circle back to this in the near
        // future to potentially improve this situation.
        //
        // see tracking issue: https://github.com/timberio/vector/issues/5887
        Ok(object
            .remove(self.path.as_ref(), false)
            .ok()
            .flatten()
            .unwrap_or(Value::Null))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::all(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use std::str::FromStr;

    #[test]
    fn del() {
        let cases = vec![
            (
                // String field exists
                btreemap! { "exists" => "value" },
                Ok(value!("value")),
                DelFn::new(Path::from("exists")),
            ),
            (
                // String field doesn't exist
                btreemap! { "exists" => "value" },
                Ok(value!(null)),
                DelFn::new(Path::from("does_not_exist")),
            ),
            (
                // Array field exists
                btreemap! { "exists" => value!([1, 2, 3]) },
                Ok(value!([1, 2, 3])),
                DelFn::new(Path::from("exists")),
            ),
            (
                // Null field exists
                btreemap! { "exists" => value!(null) },
                Ok(value!(null)),
                DelFn::new(Path::from("exists")),
            ),
            (
                // Map field exists
                btreemap! { "exists" => btreemap! { "foo" => "bar" } },
                Ok(value!(btreemap! { "foo" => "bar" })),
                DelFn::new(Path::from("exists")),
            ),
            (
                // Integer field exists
                btreemap! { "exists" => 127 },
                Ok(value!(127)),
                DelFn::new(Path::from("exists")),
            ),
            (
                // Array field exists
                btreemap! { "exists" => value!([1, 2, 3]) },
                Ok(value!(2)),
                DelFn::new(remap::Path::from_str(".exists[1]").unwrap().into()),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
