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
            source: r#"parse_query_string({"query": "foo=1&bar=2"}.query)"#,
            result: Ok(r#"
                {
                    "foo": "1",
                    "bar": "2"
                }
            "#),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let target = arguments.required_query("target")?;
        Ok(Box::new(ParseQueryStringFn { target }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "target",
            kind: kind::ANY,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseQueryStringFn {
    target: expression::Query,
}

impl ParseQueryStringFn {
    #[cfg(test)]
    fn new(path: &str) -> Self {
        use std::str::FromStr;

        Self {
            target: expression::Query::new(
                expression::Target::External,
                Path::from_str(path).unwrap(),
            ),
        }
    }
}

impl Expression for ParseQueryStringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let query_string = self.target.resolve(ctx)?.try_bytes()?;
        let result = form_urlencoded::parse(query_string.as_ref())
            .map(|(k, v)| (k.to_string(), v.into()))
            .collect();
        Ok(Value::Object(result))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object::<(), Kind>(map! {
            (): Kind::Bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    #[test]
    fn parse_query_string() {
        let cases = vec![(
            btreemap! { "query" => "foo=%2B1&bar=2" },
            Ok(value!({"foo" : "+1", "bar" : "2"})),
            ParseQueryStringFn::new(".query"),
        )];

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let mut runtime_state = vrl::state::Runtime::default();
            let mut ctx = Context::new(&mut object, &mut runtime_state);
            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));
            assert_eq!(got, exp);
        }
    }
}
