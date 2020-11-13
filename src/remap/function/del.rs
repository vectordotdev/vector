use remap::prelude::*;
use std::convert::TryFrom;

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
        macro_rules! get_variable_params {
            ($n:tt) => {{
                let mut a = vec![];
                for i in 1..=$n {
                    if let Some(arg) = arguments.optional_expr(&format!("{}", i))? {
                        a.push(arg)
                    }
                }
                a
            }};
        }

        let paths = get_variable_params!(16);

        Ok(Box::new(DelFn { paths }))
    }
}

#[derive(Debug, Clone)]
pub struct DelFn {
    paths: Vec<Box<dyn Expression>>,
}

impl Expression for DelFn {
    fn execute(
        &self,
        state: &mut state::Program,
        object: &mut dyn Object,
    ) -> Result<Option<Value>> {
        let paths = self
            .paths
            .iter()
            .filter_map(|expr| expr.execute(state, object).transpose())
            .map(|r| r.and_then(|v| Ok(String::try_from(v)?.trim_start_matches('.').to_owned())))
            .collect::<Result<Vec<String>>>()?;

        for path in paths {
            object.remove(&path, false)
        }

        Ok(None)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.paths
            .iter()
            .fold(TypeDef::default(), |acc, expression| {
                acc.merge(
                    expression
                        .type_def(state)
                        .fallible_unless(value::Kind::String),
                )
            })
            .with_constraint(value::Constraint::Any)
            .into_optional(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    remap::test_type_def![
        value_string {
            expr: |_| DelFn { paths: vec![Literal::from("foo").boxed()] },
            def: TypeDef { optional: true, constraint: value::Constraint::Any, ..Default::default() },
        }

        fallible_expression {
            expr: |_| DelFn { paths: vec![Variable::new("foo".to_owned()).boxed(), Literal::from("foo").boxed()] },
            def: TypeDef { fallible: true, optional: true, constraint: value::Constraint::Any },
        }
    ];
}
