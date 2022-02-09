use vrl::prelude::*;

struct Chars<'a> {
    bytes: &'a Bytes,
    pos: usize,
}

impl<'a> Chars<'a> {
    fn new(bytes: &'a Bytes) -> Self {
        Self { bytes, pos: 0 }
    }
}

impl<'a> Iterator for Chars<'a> {
    type Item = std::result::Result<char, u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.bytes.len() {
            return None;
        }

        let width = utf8_width::get_width(self.bytes[self.pos]);
        if width == 1 {
            self.pos += 1;
            Some(Ok(self.bytes[self.pos - 1] as char))
        } else {
            let c = std::str::from_utf8(&self.bytes[self.pos..self.pos + width]);
            match c {
                Ok(chr) => {
                    self.pos += width;
                    Some(Ok(chr.chars().next().unwrap()))
                }
                Err(_) => {
                    self.pos += 1;
                    Some(Err(self.bytes[self.pos]))
                }
            }
        }
    }
}

enum Case {
    Sensitive,
    Insensitive,
}

fn starts_with(bytes: &Bytes, starts: &Bytes, case: Case) -> bool {
    if bytes.len() < starts.len() {
        return false;
    }

    match case {
        Case::Sensitive => starts[..] == bytes[0..starts.len()],
        Case::Insensitive => {
            return Chars::new(starts)
                .zip(Chars::new(bytes))
                .all(|(a, b)| match (a, b) {
                    (Ok(a), Ok(b)) => {
                        if a.is_ascii() && b.is_ascii() {
                            a.to_ascii_lowercase() == b.to_ascii_lowercase()
                        } else {
                            a.to_lowercase().zip(b.to_lowercase()).all(|(a, b)| a == b)
                        }
                    }
                    _ => false,
                });
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StartsWith;

impl Function for StartsWith {
    fn identifier(&self) -> &'static str {
        "starts_with"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "substring",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "case_sensitive",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "case sensitive",
                source: r#"starts_with("foobar", "F")"#,
                result: Ok("false"),
            },
            Example {
                title: "case insensitive",
                source: r#"starts_with("foobar", "F", false)"#,
                result: Ok("true"),
            },
            Example {
                title: "mismatch",
                source: r#"starts_with("foobar", "bar")"#,
                result: Ok("false"),
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
        let substring = arguments.required("substring");
        let case_sensitive = arguments.optional("case_sensitive").unwrap_or(expr!(true));

        Ok(Box::new(StartsWithFn {
            value,
            substring,
            case_sensitive,
        }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, arguments: &mut VmArgumentList) -> Resolved {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments
            .optional("case_sensitive")
            .map(|arg| arg.try_boolean())
            .transpose()?
            .unwrap_or(true);
        let substring = {
            let value = substring;
            let string = value.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        let value = {
            let string = value.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        Ok(value.starts_with(&substring).into())
    }
}

#[derive(Debug, Clone)]
struct StartsWithFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Box<dyn Expression>,
}

impl Expression for StartsWithFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let case_sensitive = if self.case_sensitive.resolve(ctx)?.try_boolean()? {
            Case::Sensitive
        } else {
            Case::Insensitive
        };

        let substring = self.substring.resolve(ctx)?;
        let substring = substring.try_bytes()?;

        let value = self.value.resolve(ctx)?;
        let value = value.try_bytes()?;

        Ok(starts_with(&value, &substring, case_sensitive).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        starts_with => StartsWith;

        no {
            args: func_args![value: "foo",
                             substring: "bar"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        subset {
            args: func_args![value: "foo",
                             substring: "foobar"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        total {
            args: func_args![value: "foo",
                             substring: "foo"
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        middle {
            args: func_args![value: "foobar",
                             substring: "oba"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        start {
            args: func_args![value: "foobar",
                             substring: "foo"
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        end {
            args: func_args![value: "foobar",
                             substring: "bar"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }


        case_sensitive_same_case {
            args: func_args![value: "FOObar",
                             substring: "FOO"
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        case_sensitive_different_case {
            args: func_args![value: "foobar",
                             substring: "FOO"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        case_insensitive_different_case {
            args: func_args![value: "foobar",
                             substring: "FOO",
                             case_sensitive: false
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        unicode_same_case {
            args: func_args![value: "𛋙ၺ㚺𛋙Zonkکᤊᰙ𛋙Ꮺ믚㋫𐠘𒃪𖾛𞺘ᰙꢝⶺ觨⨙ઉzook",
                             substring: "𛋙ၺ㚺𛋙Zonkکᤊᰙ𛋙",
                             case_sensitive: true
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        unicode_sensitive_different_case {
            args: func_args![value: "ξ𛋙ၺ㚺𛋙Zonkکᤊᰙ𛋙Ꮺ믚㋫𐠘𒃪𖾛𞺘ᰙꢝⶺ觨⨙ઉzook",
                             substring: "Ξ𛋙ၺ㚺𛋙Zonkکᤊᰙ𛋙",
                             case_sensitive: true
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        unicode_insensitive_different_case {
            args: func_args![value: "ξ𛋙ၺ㚺𛋙Zonkکᤊᰙ𛋙Ꮺ믚㋫𐠘𒃪𖾛𞺘ᰙꢝⶺ觨⨙ઉzook",
                             substring: "Ξ𛋙ၺ㚺𛋙Zonkکᤊᰙ𛋙",
                             case_sensitive: false
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
