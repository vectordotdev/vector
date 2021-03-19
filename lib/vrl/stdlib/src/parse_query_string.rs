use bytes::Buf;
use std::collections::btree_map::Entry;
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
        let mut result: BTreeMap<String, Value> = BTreeMap::new();
        let parsed = form_urlencoded::parse(query_string.bytes());
        for (k, v) in parsed {
            let value = v.as_ref().into();
            let entry = result.entry(k.as_ref().to_owned());
            match entry {
                Entry::Occupied(mut e) => {
                    if e.get().is_array() {
                        e.get_mut().as_array_mut().unwrap().push(value);
                    } else {
                        let prev_value = e.get().to_owned();
                        result.insert(k.as_ref().into(), vec![prev_value, value].into());
                    }
                }
                Entry::Vacant(e) => {
                    e.insert(value);
                }
            }
        }
        Ok(Value::Object(result))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().object::<(), Kind>(type_def())
    }
}

fn type_def() -> BTreeMap<(), Kind> {
    map! {
        (): Kind::Bytes | Kind::Array,
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

        multiple_values {
            args: func_args![value: value!("foo=bar&foo=xyz")],
            want: Ok(value!({
                foo: ["bar", "xyz"],
            })),
            tdef: TypeDef::new().infallible().object::<(), Kind>(type_def()),
        }

        empty_key {
            args: func_args![value: value!("=&=")],
            want: Ok(value!({
                "": ["", ""],
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
