use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToSyslogFacility;

impl Function for ToSyslogFacility {
    fn identifier(&self) -> &'static str {
        "to_syslog_facility"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::INTEGER,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "valid",
                source: "to_syslog_facility!(0)",
                result: Ok("kern"),
            },
            Example {
                title: "invalid",
                source: "to_syslog_facility!(500)",
                result: Err(
                    r#"function call error for "to_syslog_facility" at (0:24): facility code 500 not valid"#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToSyslogFacilityFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToSyslogFacilityFn {
    value: Box<dyn Expression>,
}

impl Expression for ToSyslogFacilityFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.unwrap_integer();

        // Facility codes: https://en.wikipedia.org/wiki/Syslog#Facility
        let code = match value {
            0 => "kern",
            1 => "user",
            2 => "mail",
            3 => "daemon",
            4 => "auth",
            5 => "syslog",
            6 => "lpr",
            7 => "news",
            8 => "uucp",
            9 => "cron",
            10 => "authpriv",
            11 => "ftp",
            12 => "ntp",
            13 => "security",
            14 => "console",
            15 => "solaris-cron",
            16 => "local0",
            17 => "local1",
            18 => "local2",
            19 => "local3",
            20 => "local4",
            21 => "local5",
            22 => "local6",
            23 => "local7",
            _ => return Err(format!("facility code {} not valid", value).into()),
        };

        Ok(code.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     test_type_def![value_non_integer_fallible {
//         expr: |_| ToSyslogFacilityFn {
//             value: Literal::from("foo").boxed(),
//         },
//         def: TypeDef {
//             fallible: true,
//             kind: Kind::Bytes,
//             ..Default::default()
//         },
//     }];

//     test_function![
//         to_syslog_facility => ToSyslogFacility;

//         kern {
//             args: func_args![value: value!(0)],
//             want: Ok(value!("kern")),
//         }

//         user {
//             args: func_args![value: value!(1)],
//             want: Ok(value!("user")),
//         }

//         mail {
//             args: func_args![value: value!(2)],
//             want: Ok(value!("mail")),
//         }

//         daemon {
//             args: func_args![value: value!(3)],
//             want: Ok(value!("daemon")),
//         }

//         auth {
//             args: func_args![value: value!(4)],
//             want: Ok(value!("auth")),
//         }

//         syslog {
//             args: func_args![value: value!(5)],
//             want: Ok(value!("syslog")),
//         }

//         lpr {
//             args: func_args![value: value!(6)],
//             want: Ok(value!("lpr")),
//         }

//         news {
//             args: func_args![value: value!(7)],
//             want: Ok(value!("news")),
//         }

//         uucp {
//             args: func_args![value: value!(8)],
//             want: Ok(value!("uucp")),
//         }

//         cron {
//             args: func_args![value: value!(9)],
//             want: Ok(value!("cron")),
//         }

//         authpriv {
//             args: func_args![value: value!(10)],
//             want: Ok(value!("authpriv")),
//         }

//         ftp {
//             args: func_args![value: value!(11)],
//             want: Ok(value!("ftp")),
//         }

//         ntp {
//             args: func_args![value: value!(12)],
//             want: Ok(value!("ntp")),
//         }

//         security {
//             args: func_args![value: value!(13)],
//             want: Ok(value!("security")),
//         }

//         console {
//             args: func_args![value: value!(14)],
//             want: Ok(value!("console")),
//         }

//         solaris_cron {
//             args: func_args![value: value!(15)],
//             want: Ok(value!("solaris-cron")),
//         }

//         local0 {
//             args: func_args![value: value!(16)],
//             want: Ok(value!("local0")),
//         }

//         local1 {
//             args: func_args![value: value!(17)],
//             want: Ok(value!("local1")),
//         }

//         local2 {
//             args: func_args![value: value!(18)],
//             want: Ok(value!("local2")),
//         }

//         local3 {
//             args: func_args![value: value!(19)],
//             want: Ok(value!("local3")),
//         }

//         local4 {
//             args: func_args![value: value!(20)],
//             want: Ok(value!("local4")),
//         }

//         local5 {
//             args: func_args![value: value!(21)],
//             want: Ok(value!("local5")),
//         }

//         local6 {
//             args: func_args![value: value!(22)],
//             want: Ok(value!("local6")),
//         }

//         local7 {
//             args: func_args![value: value!(23)],
//             want: Ok(value!("local7")),
//         }

//         invalid_facility_larger_int {
//             args: func_args![value: value!(475)],
//             want: Err("function call error: facility code 475 not valid"),
//         }

//         invalid_facility_negative_int {
//             args: func_args![value: value!(-1)],
//             want: Err("function call error: facility code -1 not valid"),
//         }

//         invalid_facility_non_int {
//             args: func_args![value: value!("nope")],
//             want: Err(r#"value error: expected "integer", got "string""#),
//         }
//     ];
// }
