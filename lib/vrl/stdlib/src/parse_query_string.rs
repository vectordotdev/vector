use std::collections::BTreeMap;

use url::form_urlencoded;
use vrl::prelude::*;

fn parse_query_string(bytes: Value) -> Resolved {
    let bytes = bytes.try_bytes()?;
    let mut query_string = bytes.as_ref();
    if !query_string.is_empty() && query_string[0] == b'?' {
        query_string = &query_string[1..];
    }
    let mut result = BTreeMap::new();
    let parsed = form_urlencoded::parse(query_string);
    for (k, value) in parsed {
        let value = value.as_ref();
        result
            .entry(k.into_owned())
            .and_modify(|v| {
                match v {
                    Value::Array(v) => {
                        v.push(value.into());
                    }
                    v => {
                        *v = Value::Array(vec![v.to_owned(), value.into()]);
                    }
                };
            })
            .or_insert_with(|| value.into());
    }
    Ok(result.into())
}

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

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
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

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        parse_query_string(value)
    }
}

#[derive(Debug, Clone)]
struct ParseQueryStringFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseQueryStringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        parse_query_string(bytes)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::object(inner_kind())
    }
}

fn inner_kind() -> Collection<Field> {
    Collection::from_unknown(Kind::bytes().or_array(Collection::any()))
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
            tdef: TypeDef::object(inner_kind()),
        }

        multiple_values {
            args: func_args![value: value!("foo=bar&foo=xyz")],
            want: Ok(value!({
                foo: ["bar", "xyz"],
            })),
            tdef: TypeDef::object(inner_kind()),
        }

        ruby_on_rails_multiple_values {
            args: func_args![value: value!("?foo%5b%5d=bar&foo%5b%5d=xyz")],
            want: Ok(value!({
                "foo[]": ["bar", "xyz"],
            })),
            tdef: TypeDef::object(inner_kind()),
        }

        empty_key {
            args: func_args![value: value!("=&=")],
            want: Ok(value!({
                "": ["", ""],
            })),
            tdef: TypeDef::object(inner_kind()),
        }

        single_key {
            args: func_args![value: value!("foo")],
            want: Ok(value!({
                foo: "",
            })),
            tdef: TypeDef::object(inner_kind()),
        }

        empty {
            args: func_args![value: value!("")],
            want: Ok(value!({})),
            tdef: TypeDef::object(inner_kind()),
        }

        starts_with_question_mark {
            args: func_args![value: value!("?foo=bar")],
            want: Ok(value!({
                foo: "bar",
            })),
            tdef: TypeDef::object(inner_kind()),
        }
    ];
}
