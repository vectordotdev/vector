use remap::prelude::*;
use std::str::FromStr;

#[derive(Clone, Copy, Debug)]
pub struct EncodeBase64;

impl Function for EncodeBase64 {
    fn identifier(&self) -> &'static str {
        "encode_base64"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "padding",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
            Parameter {
                keyword: "charset",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            }
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let padding = arguments.optional("padding").map(Expr::boxed);
        let charset = arguments
            .optional_enum("charset", &Charset::all_str())?
            .map(|c| Charset::from_str(&c).expect("validated enum"))
            .unwrap_or_default();

        Ok(Box::new(EncodeBase64Fn { value, padding, charset }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Charset {
    Standard,
    UrlSafe,
}

impl Default for Charset {
    fn default() -> Self {
        Self::Standard
    }
}

impl Charset {
    fn all_str() -> Vec<&'static str> {
        use Charset::*;

        vec![Standard, UrlSafe]
            .into_iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
    }

    const fn as_str(self) -> &'static str {
        use Charset::*;

        match self {
            Standard => "standard",
            UrlSafe => "url_safe",
        }
    }
}

impl Into<base64::CharacterSet> for Charset {
    fn into(self) -> base64::CharacterSet {
        use Charset::*;

        match self {
            Standard => base64::CharacterSet::Standard,
            UrlSafe => base64::CharacterSet::UrlSafe,
        }
    }
}

impl FromStr for Charset {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Charset::*;

        match s {
            "standard" => Ok(Standard),
            "url_safe" => Ok(UrlSafe),
            _ => Err("unknown charset"),
        }
    }
}

#[derive(Clone, Debug)]
struct EncodeBase64Fn {
    value: Box<dyn Expression>,
    padding: Option<Box<dyn Expression>>,
    charset: Charset,
}

impl Expression for EncodeBase64Fn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?.try_bytes()?;

        let padding = self
            .padding
            .as_ref()
            .map(|p| {
                p.execute(state, object)
                    .and_then(|v| Value::try_boolean(v).map_err(Into::into))
            })
            .transpose()?
            .unwrap_or(true);

        let config = base64::Config::new(self.charset.into(), padding);

        Ok(base64::encode_config(value, config).into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let padding_def = self
            .padding
            .as_ref()
            .map(|padding| padding.type_def(state).fallible_unless(Kind::Boolean));

        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .merge_optional(padding_def)
            .with_constraint(Kind::Bytes)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use value::Kind;

    test_type_def![
        value_string_padding_unspecified_infallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from("foo").boxed(),
                padding: None,
                charset: Charset::default(),
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        value_string_padding_boolean_infallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from("foo").boxed(),
                padding: Some(Literal::from(false).boxed()),
                charset: Charset::default(),
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        value_string_padding_non_boolean_fallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from("foo").boxed(),
                padding: Some(Literal::from("foo").boxed()),
                charset: Charset::default(),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string_fallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from(127).boxed(),
                padding: Some(Literal::from(true).boxed()),
                charset: Charset::default(),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        both_types_wrong_fallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from(127).boxed(),
                padding: Some(Literal::from("foo").boxed()),
                charset: Charset::default(),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    test_function![
        encode_base64 => EncodeBase64;

        string_value_with_padding {
            args: func_args![value: value!("some string value"), padding: value!(true)],
            want: Ok(value!("c29tZSBzdHJpbmcgdmFsdWU=")),
        }

        string_value_with_default_padding {
            args: func_args![value: value!("some string value")],
            want: Ok(value!("c29tZSBzdHJpbmcgdmFsdWU=")),
        }

        string_value_no_padding {
            args: func_args![value: value!("some string value"), padding: value!(false)],
            want: Ok(value!("c29tZSBzdHJpbmcgdmFsdWU")),
        }

        empty_string_value {
            args: func_args![value: value!("")],
            want: Ok(value!("")),
        }
    ];
}
