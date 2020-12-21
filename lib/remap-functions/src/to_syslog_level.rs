use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToSyslogLevel;

impl Function for ToSyslogLevel {
    fn identifier(&self) -> &'static str {
        "to_syslog_level"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Integer(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ToSyslogLevelFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToSyslogLevelFn {
    value: Box<dyn Expression>,
}

impl Expression for ToSyslogLevelFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?.try_integer()?;

        // Severity levels: https://en.wikipedia.org/wiki/Syslog#Severity_level
        let level = match value {
            0 => Ok("emerg"),
            1 => Ok("alert"),
            2 => Ok("crit"),
            3 => Ok("err"),
            4 => Ok("warning"),
            5 => Ok("notice"),
            6 => Ok("info"),
            7 => Ok("debug"),
            _ => Err(Error::from(format!("severity level {} not valid", value))),
        };

        match level {
            Ok(level) => Ok(Value::from(level)),
            Err(e) => Err(e),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Integer)
            .with_constraint(Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    test_type_def![
        value_integer_non_fallible {
            expr: |_| ToSyslogLevelFn {
                value: Literal::from(3).boxed(),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        value_non_integer_fallible {
            expr: |_| ToSyslogLevelFn {
                value: Literal::from("foo").boxed(),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }
    ];

    test_function![
        to_syslog_level => ToSyslogLevel;

        emergency {
            args: func_args![value: value!(0)],
            want: Ok(value!("emerg")),
        }

        alert {
            args: func_args![value: value!(1)],
            want: Ok(value!("alert")),
        }

        critical {
            args: func_args![value: value!(2)],
            want: Ok(value!("crit")),
        }

        error {
            args: func_args![value: value!(3)],
            want: Ok(value!("err")),
        }

        warning {
            args: func_args![value: value!(4)],
            want: Ok(value!("warning")),
        }

        notice {
            args: func_args![value: value!(5)],
            want: Ok(value!("notice")),
        }

        informational {
            args: func_args![value: value!(6)],
            want: Ok(value!("info")),
        }

        debug {
            args: func_args![value: value!(7)],
            want: Ok(value!("debug")),
        }

        invalid_severity_next_int {
            args: func_args![value: value!(8)],
            want: Err("function call error: severity level 8 not valid"),
        }

        invalid_severity_larger_int {
            args: func_args![value: value!(475)],
            want: Err("function call error: severity level 475 not valid"),
        }

        invalid_severity_negative_int {
            args: func_args![value: value!(-1)],
            want: Err("function call error: severity level -1 not valid"),
        }
    ];
}
