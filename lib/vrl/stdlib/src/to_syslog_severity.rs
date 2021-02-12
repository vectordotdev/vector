use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToSyslogSeverity;

impl Function for ToSyslogSeverity {
    fn identifier(&self) -> &'static str {
        "to_syslog_severity"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "valid",
                source: "to_syslog_severity!(s'crit')",
                result: Ok("2"),
            },
            Example {
                title: "invalid",
                source: "to_syslog_severity!(s'foobar')",
                result: Err(
                    r#"function call error for "to_syslog_severity" at (0:30): syslog level foobar not valid"#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToSyslogSeverityFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToSyslogSeverityFn {
    value: Box<dyn Expression>,
}

impl Expression for ToSyslogSeverityFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let level = self.value.resolve(ctx)?;
        let level = level.unwrap_bytes_utf8_lossy();

        // Severity levels: https://en.wikipedia.org/wiki/Syslog#Severity_level
        let severity = match &level[..] {
            "emerg" | "panic" => 0,
            "alert" => 1,
            "crit" => 2,
            "err" | "error" => 3,
            "warning" | "warn" => 4,
            "notice" => 5,
            "info" => 6,
            "debug" => 7,
            _ => return Err(format!("syslog level {} not valid", level).into()),
        };

        Ok(severity.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().integer()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     test_type_def![value_not_string_fallible {
//         expr: |_| ToSyslogSeverityFn {
//             value: Literal::from(27).boxed(),
//         },
//         def: TypeDef {
//             fallible: true,
//             kind: Kind::Integer,
//             ..Default::default()
//         },
//     }];

//     test_function![
//         to_level => ToSyslogSeverity;

//         emergency {
//             args: func_args![value: value!("emerg")],
//             want: Ok(value!(0)),
//         }

//         alert {
//             args: func_args![value: value!("alert")],
//             want: Ok(value!(1)),
//         }

//         critical {
//             args: func_args![value: value!("crit")],
//             want: Ok(value!(2)),
//         }

//         error {
//             args: func_args![value: value!("err")],
//             want: Ok(value!(3)),
//         }

//         warning {
//             args: func_args![value: value!("warn")],
//             want: Ok(value!(4)),
//         }

//         notice {
//             args: func_args![value: value!("notice")],
//             want: Ok(value!(5)),
//         }

//         informational {
//             args: func_args![value: value!("info")],
//             want: Ok(value!(6)),
//         }

//         debug {
//             args: func_args![value: value!("debug")],
//             want: Ok(value!(7)),
//         }

//         invalid_level_1 {
//             args: func_args![value: value!("oopsie")],
//             want: Err("function call error: level oopsie not valid"),
//         }

//         invalid_level_2 {
//             args: func_args![value: value!("aww schucks")],
//             want: Err("function call error: level aww schucks not valid"),
//         }
//     ];
// }
