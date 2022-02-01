use chrono::{
    format::{strftime::StrftimeItems, Item},
    DateTime, Utc,
};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct FormatTimestamp;

impl Function for FormatTimestamp {
    fn identifier(&self) -> &'static str {
        "format_timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::TIMESTAMP,
                required: true,
            },
            Parameter {
                keyword: "format",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let format = arguments.required("format");

        Ok(Box::new(FormatTimestampFn { value, format }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "format timestamp",
            source: r#"format_timestamp!(t'2021-02-10T23:32:00+00:00', "%d %B %Y %H:%M")"#,
            result: Ok("10 February 2021 23:32"),
        }]
    }
}

#[derive(Debug, Clone)]
struct FormatTimestampFn {
    value: Box<dyn Expression>,
    format: Box<dyn Expression>,
}

impl Expression for FormatTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.format.resolve(ctx)?.try_bytes()?;
        let format = String::from_utf8_lossy(&bytes);
        let ts = self.value.resolve(ctx)?.try_timestamp()?;

        try_format(&ts, &format).map(Into::into)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

fn try_format(dt: &DateTime<Utc>, format: &str) -> Result<String> {
    let items = StrftimeItems::new(format)
        .map(|item| match item {
            Item::Error => Err("invalid format".into()),
            _ => Ok(item),
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(dt.format_with_items(items.into_iter()).to_string())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    test_function![
        format_timestamp => FormatTimestamp;

        invalid {
            args: func_args![value: Utc.timestamp(10, 0),
                             format: "%Q INVALID"],
            want: Err("invalid format"),
            tdef: TypeDef::new().fallible().bytes(),
        }

        valid_secs {
            args: func_args![value: Utc.timestamp(10, 0),
                             format: "%s"],
            want: Ok(value!("10")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        date {
            args: func_args![value: Utc.timestamp(10, 0),
                             format: "%+"],
            want: Ok(value!("1970-01-01T00:00:10+00:00")),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];
}
