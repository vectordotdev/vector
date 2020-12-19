use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Del;

impl Function for Del {
    fn identifier(&self) -> &'static str {
        "del"
    }

    fn parameters(&self) -> &'static [Parameter] {
        generate_param_list! {
            accepts = |_| true,
            required = false,
            keywords = [
                "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
            ],
        }
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let mut paths = vec![];
        paths.push(arguments.required_path("1")?);

        for i in 2..=16 {
            if let Some(path) = arguments.optional_path(&format!("{}", i))? {
                paths.push(path)
            }
        }

        Ok(Box::new(DelFn { paths }))
    }
}

#[derive(Debug, Clone)]
pub struct DelFn {
    paths: Vec<Path>,
}

impl Expression for DelFn {
    fn execute(&self, _: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        self.paths
            .iter()
            .try_for_each(|path| object.remove(path.as_ref(), false))?;

        Ok(Value::Null)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            fallible: true,
            kind: value::Kind::Null,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_type_def![static_type_def {
        expr: |_| DelFn {
            paths: vec![Path::from("foo")]
        },
        def: TypeDef {
            fallible: true,
            kind: value::Kind::Null,
            ..Default::default()
        },
    }];
}
