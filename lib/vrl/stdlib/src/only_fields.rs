use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct OnlyFields;

impl Function for OnlyFields {
    fn identifier(&self) -> &'static str {
        "only_fields"
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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let mut paths = vec![];
        paths.push(arguments.required_path("1")?);

        for i in 2..=16 {
            if let Some(path) = arguments.optional_path(&format!("{}", i))? {
                paths.push(path)
            }
        }

        Ok(Box::new(OnlyFieldsFn { paths }))
    }
}

#[derive(Debug, Clone)]
pub struct OnlyFieldsFn {
    paths: Vec<Path>,
}

impl Expression for OnlyFieldsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let paths = self.paths.iter().map(Path::to_string).collect::<Vec<_>>();

        object
            .paths()?
            .iter()
            .map(|path| (path, path.to_string()))
            .filter(|(_, key)| paths.iter().find(|p| key.starts_with(p.as_str())).is_none())
            .try_for_each(|(path, _)| object.remove(&path, true).map(|_| ()))?;

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
        expr: |_| OnlyFieldsFn {
            paths: vec![Path::from("foo")]
        },
        def: TypeDef {
            fallible: true,
            kind: value::Kind::Null,
            ..Default::default()
        },
    }];
}
