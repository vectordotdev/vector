use remap::{ArgumentList, Expression, Function, Object, Parameter, Result, State, Value};
use std::convert::TryFrom;

#[derive(Debug)]
pub struct Del;

impl Function for Del {
    fn identifier(&self) -> &'static str {
        "del"
    }

    fn parameters(&self) -> &'static [Parameter] {
        // workaround for missing variable argument length.
        //
        // We'll come up with a nicer solution at some point. It took Rust five
        // years to support [0; 34].
        macro_rules! set_variable_params {
            ($($n:tt),+ $(,)?) => (
                &[$(Parameter {
                        keyword: stringify!($n),
                        accepts: |v| matches!(v, Value::String(_)),
                        required: false,
                    }),+]
            );
        }

        set_variable_params!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16)
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

#[derive(Debug)]
pub struct DelFn {
    paths: Vec<Box<dyn Expression>>,
}

impl Expression for DelFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let paths = self
            .paths
            .iter()
            .filter_map(|expr| expr.execute(state, object).transpose())
            .map(|r| r.and_then(|v| Ok(String::try_from(v)?.trim_start_matches(".").to_owned())))
            .collect::<Result<Vec<String>>>()?;

        for path in paths {
            object.remove(&path, false)
        }

        Ok(None)
    }
}
