use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToSyslogFacility;

impl Function for ToSyslogFacility {
    fn identifier(&self) -> &'static str {
        "to_syslog_facility"
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

        Ok(Box::new(ToSyslogFacilityFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToSyslogFacilityFn {
    value: Box<dyn Expression>,
}

impl Expression for ToSyslogFacilityFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?.try_integer()?;

        // Facility codes: https://en.wikipedia.org/wiki/Syslog#Facility
        let code = match value {
            0 => Ok("kern"),
            1 => Ok("user"),
            2 => Ok("mail"),
            3 => Ok("daemon"),
            4 => Ok("auth"),
            5 => Ok("syslog"),
            6 => Ok("lpr"),
            7 => Ok("news"),
            8 => Ok("uucp"),
            9 => Ok("cron"),
            10 => Ok("authpriv"),
            11 => Ok("ftp"),
            12 => Ok("ntp"),
            13 => Ok("security"),
            14 => Ok("console"),
            15 => Ok("cron"),
            16 => Ok("local0"),
            17 => Ok("local1"),
            18 => Ok("local2"),
            19 => Ok("local3"),
            20 => Ok("local4"),
            21 => Ok("local5"),
            22 => Ok("local6"),
            23 => Ok("local7"),
            _ => Err(Error::from(format!("severity facility {} not valid", value))),
        };

        match code {
            Ok(code) => Ok(Value::from(code)),
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
            expr: |_| ToSyslogFacilityFn {
                value: Literal::from(3).boxed(),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        value_non_integer_fallible {
            expr: |_| ToSyslogFacilityFn {
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
        to_syslog_facility => ToSyslogFacility;

        kern {
            args: func_args![value: value!(0)],
            want: Ok(value!("kern")),
        }

        user {
            args: func_args![value: value!(1)],
            want: Ok(value!("user")),
        }

        mail {
            args: func_args![value: value!(2)],
            want: Ok(value!("mail")),
        }

        daemon {
            args: func_args![value: value!(3)],
            want: Ok(value!("daemon")),
        }

        auth {
            args: func_args![value: value!(4)],
            want: Ok(value!("auth")),
        }

        syslog {
            args: func_args![value: value!(5)],
            want: Ok(value!("syslog")),
        }

        lpr {
            args: func_args![value: value!(6)],
            want: Ok(value!("lpr")),
        }

        news {
            args: func_args![value: value!(7)],
            want: Ok(value!("news")),
        }

        uucp {
            args: func_args![value: value!(8)],
            want: Ok(value!("uucp")),
        }

        cron {
            args: func_args![value: value!(9)],
            want: Ok(value!("cron")),
        }

        authpriv {
            args: func_args![value: value!(10)],
            want: Ok(value!("authpriv")),
        }

        ftp {
            args: func_args![value: value!(11)],
            want: Ok(value!("ftp")),
        }

        ntp {
            args: func_args![value: value!(12)],
            want: Ok(value!("ntp")),
        }

        security {
            args: func_args![value: value!(13)],
            want: Ok(value!("security")),
        }

        console {
            args: func_args![value: value!(14)],
            want: Ok(value!("console")),
        }

        cron {
            args: func_args![value: value!(15)],
            want: Ok(value!("cron")),
        }

        local0 {
            args: func_args![value: value!(16)],
            want: Ok(value!("local0")),
        }

        local1 {
            args: func_args![value: value!(17)],
            want: Ok(value!("local1")),
        }

        local2 {
            args: func_args![value: value!(18)],
            want: Ok(value!("local2")),
        }

        local3 {
            args: func_args![value: value!(19)],
            want: Ok(value!("local3")),
        }

        local4 {
            args: func_args![value: value!(20)],
            want: Ok(value!("local4")),
        }

        local5 {
            args: func_args![value: value!(21)],
            want: Ok(value!("local5")),
        }

        local6 {
            args: func_args![value: value!(22)],
            want: Ok(value!("local6")),
        }

        local7 {
            args: func_args![value: value!(23)],
            want: Ok(value!("local7")),
        }

        invalid_severity_larger_int {
            args: func_args![value: value!(475)],
            want: Err("function call error: facility code 475 not valid"),
        }

        invalid_severity_negative_int {
            args: func_args![value: value!(-1)],
            want: Err("function call error: facility code -1 not valid"),
        }
    ];
}
