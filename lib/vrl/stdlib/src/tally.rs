use vrl::prelude::*;

use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug)]
pub struct Tally;

impl Function for Tally {
    fn identifier(&self) -> &'static str {
        "tally"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "tally",
            source: r#"tally(["foo", "bar", "foo", "baz"])"#,
            result: Ok(r#"{"foo": 2, "bar": 1, "baz": 1]"#),
        }]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(TallyFn { value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ARRAY,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TallyFn {
    value: Box<dyn Expression>,
}

impl Expression for TallyFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_array()?;

        let map: BTreeMap<_, _> = value
            .into_iter()
            .fold(HashMap::<_, _>::new(), |mut m, x| {
                *m.entry(x.to_string()).or_insert(0) += 1;
                m
            })
            .into_iter()
            .map(|(k, v)| (k, Value::from(v)))
            .collect();

        Ok(map.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().object::<(), Kind>(map! { (): Kind::all() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        tally => Tally;

        default {
            args: func_args![
                value: value!(["bar", "foo", "baz", "foo"]),
            ],
            want: Ok(value!({"bar": 1, "foo": 2, "baz": 1})),
            tdef: TypeDef::new().object::<(), Kind>(map! { (): Kind::all() }),
        }

        mixed_values {
            args: func_args![
                value: value!(["foo", [1,2,3], "123abc", 1, true, [1,2,3], "foo", true, 1]),
            ],
            want: Ok(value!({"foo": 2, "[1,2,3]": 2, "123abc": 1, "1": 2, "true": 2})),
            tdef: TypeDef::new().object::<(), Kind>(map! { (): Kind::all() }),
        }
    ];
}
