use ::value::Value;
use csv::ReaderBuilder;
use vrl::prelude::*;

fn parse_csv(csv_string: Value, delimiter: Value) -> Resolved {
    let csv_string = csv_string.try_bytes()?;
    let delimiter = delimiter.try_bytes()?;
    if delimiter.len() != 1 {
        return Err("delimiter must be a single character".into());
    }
    let delimiter = delimiter[0];
    let reader = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .from_reader(&*csv_string);
    reader
        .into_byte_records()
        .next()
        .transpose()
        .map_err(|err| format!("invalid csv record: {err}").into()) // shouldn't really happen
        .map(|record| {
            record
                .map(|record| {
                    record
                        .iter()
                        .map(|x| Bytes::copy_from_slice(x).into())
                        .collect::<Vec<Value>>()
                })
                .unwrap_or_default()
                .into()
        })
}

#[derive(Clone, Copy, Debug)]
pub struct ParseCsv;

impl Function for ParseCsv {
    fn identifier(&self) -> &'static str {
        "parse_csv"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse a single CSV formatted row",
            source: r#"parse_csv!(s'foo,bar,"foo "", bar"')"#,
            result: Ok(r#"["foo", "bar", "foo \", bar"]"#),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let delimiter = arguments.optional("delimiter").unwrap_or(expr!(","));
        Ok(ParseCsvFn { value, delimiter }.as_expr())
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "delimiter",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }
}

#[derive(Debug, Clone)]
struct ParseCsvFn {
    value: Box<dyn Expression>,
    delimiter: Box<dyn Expression>,
}

impl FunctionExpression for ParseCsvFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let csv_string = self.value.resolve(ctx)?;
        let delimiter = self.delimiter.resolve(ctx)?;

        parse_csv(csv_string, delimiter)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::array(inner_kind()).fallible()
    }
}

#[inline]
fn inner_kind() -> Collection<Index> {
    let mut v = Collection::any();
    v.set_unknown(Kind::bytes());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_csv => ParseCsv;

        valid {
            args: func_args![value: value!("foo,bar,\"foo \"\", bar\"")],
            want: Ok(value!(["foo", "bar", "foo \", bar"])),
            tdef: TypeDef::array(inner_kind()).fallible(),
        }

        invalid_utf8 {
            args: func_args![value: value!(Bytes::copy_from_slice(&b"foo,b\xFFar"[..]))],
            want: Ok(value!(vec!["foo".into(), value!(Bytes::copy_from_slice(&b"b\xFFar"[..]))])),
            tdef: TypeDef::array(inner_kind()).fallible(),
        }

        custom_delimiter {
            args: func_args![value: value!("foo bar"), delimiter: value!(" ")],
            want: Ok(value!(["foo", "bar"])),
            tdef: TypeDef::array(inner_kind()).fallible(),
        }

        invalid_delimiter {
            args: func_args![value: value!("foo bar"), delimiter: value!(",,")],
            want: Err("delimiter must be a single character"),
            tdef: TypeDef::array(inner_kind()).fallible(),
        }

        single_value {
            args: func_args![value: value!("foo")],
            want: Ok(value!(["foo"])),
            tdef: TypeDef::array(inner_kind()).fallible(),
        }

        empty_string {
            args: func_args![value: value!("")],
            want: Ok(value!([])),
            tdef: TypeDef::array(inner_kind()).fallible(),
        }

        multiple_lines {
            args: func_args![value: value!("first,line\nsecond,line,with,more,fields")],
            want: Ok(value!(["first", "line"])),
            tdef: TypeDef::array(inner_kind()).fallible(),
        }
    ];
}
