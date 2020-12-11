use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToLevel;

impl Function for ToLevel {
    fn identifier(&self) -> &'static str {
        "to_level"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "severity",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: true,
            }
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let severity = arguments.required("severity")?.boxed();

        Ok(Box::new(ToLevelFn { severity }))
    }
}

#[derive(Debug, Clone)]
struct ToLevelFn {
    severity: Box<dyn Expression>,
}


impl Expression for ToLevelFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let severity = self.severity.execute(state, object)?.try_integer()?;

        // Severity levels: https://en.wikipedia.org/wiki/Syslog#Severity_level
        let level = match severity {
            0 => Ok("emerg"),
            1 => Ok("alert"),
            2 => Ok("crit"),
            3 => Ok("err"),
            4 => Ok("warn"),
            5 => Ok("notice"),
            6 => Ok("info"),
            7 => Ok("debug"),
            _ => Err(Error::from(format!("severity level {} not valid", severity))),
        };

        match level {
            Ok(level) => Ok(Value::from(level)),
            Err(e) => Err(e),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::Bytes,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_level => ToLevel;

        emergency {
            args: func_args![severity: value!([0])],
            want: Ok(value!(["emerg"])),
        }

        alert {
            args: func_args![severity: value![1]],
            want: Ok(value!(["alert"])),
        }

        critical {
            args: func_args![severity: value![2]],
            want: Ok(value!(["crit"])),
        }

        error {
            args: func_args![severity: value![3]],
            want: Ok(value!(["err"])),
        }

        warning {
            args: func_args![severity: value![4]],
            want: Ok(value!(["warn"])),
        }

        notice {
            args: func_args![severity: value![5]],
            want: Ok(value!(["notice"])),
        }

        informational {
            args: func_args![severity: value![6]],
            want: Ok(value!(["info"])),
        }

        debug {
            args: func_args![severity: value![7]],
            want: Ok(value!(["debug"])),
        }

        invalid_severity_1 {
            args: func_args![severity: value![8]],
            want: Err("severity level 8 not valid"),
        }

        invalid_severity_2 {
            args: func_args![severity: value![475]],
            want: Err("severity level 475 not valid"),
        }
    ];
}
