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
            source: r#"parse_query_string("foo=1&bar=2")"#,
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
            kind: kind::BYTES,
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
        TypeDef::new().infallible().object::<(), Kind>(type_def())
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

        complete {
            args: func_args![value: value!("foo=%2B1&bar=2&xyz=&abc")],
            want: Ok(value!({
                foo: "+1",
                bar: "2",
                xyz: "",
                abc: "",
            })),
            tdef: TypeDef::new().infallible().object::<(), Kind>(type_def()),
        }

        single_key {
            args: func_args![value: value!("foo")],
            want: Ok(value!({
                foo: "",
            })),
            tdef: TypeDef::new().infallible().object::<(), Kind>(type_def()),
        }

        empty {
            args: func_args![value: value!("")],
            want: Ok(value!({})),
            tdef: TypeDef::new().infallible().object::<(), Kind>(type_def()),
        }
    ];
}
