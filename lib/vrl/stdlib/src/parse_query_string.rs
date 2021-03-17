use std::collections::BTreeMap;
use url::form_urlencoded;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseQueryString;

impl Function for ParseQueryString {
    fn identifier(&self) -> &'static str {
        "parse_query_string"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse query string",
            source: r#"parse_query_string!("foo=1&bar=2")"#,
            result: Ok(r#"
                {
                    "foo": "1",
                    "bar": "2"
                }
            "#),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        Ok(Box::new(ParseQueryStringFn { value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseQueryStringFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseQueryStringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let query_string = self.value.resolve(ctx)?.try_bytes()?;
        let result = form_urlencoded::parse(query_string.as_ref())
            .map(|(k, v)| (k.to_string(), v.into()))
            .collect();
        Ok(Value::Object(result))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object::<(), Kind>(type_def())
    }
}

fn type_def() -> BTreeMap<(), Kind> {
    map! {
        (): Kind::Bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_query_string => ParseQueryString;

        type_def {
            args: func_args![value: value!("foo=%2B1&bar=2")],
            want: Ok(value!({
                foo: "+1",
                bar: "2",
            })),
            tdef: TypeDef::new().fallible().object::<(), Kind>(type_def()),
        }
    ];
}
