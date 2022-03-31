use std::str::Split;
use vrl::prelude::expression::Expr;
use vrl::prelude::value::Error;
use vrl::prelude::*;

fn next_part(parts: &mut Split<&str>, algorithm: &str) -> Result<&str> {
    match algorithm_parts.next() {
        None => Err(format!("Invalid algorithm: {}", algorithm)).into(),
        Some(part) => Ok(part),
    }
}

fn encrypt(plaintext: Value, algorithm: Value, key: Value) -> Resolved {
    let plaintext = plaintext.try_bytes()?;
    let algorithm = algorithm.try_bytes_utf8_lossy()?.as_ref().to_uppercase();

    let mut algorithm_parts = algorithm.split("-");

    match next_part(&mut algorithm_parts, &algorithm)? {
        "AES" => match next_part(&mut algorithm_parts, &algorithm)? {
            "256" => match next_part(&mut algorithm_parts, &algorithm)? {
                "ECB" => match next_part(&mut algorithm_parts, &algorithm)? {
                    "PKCS7" => {}
                    other => return Err(format!("Invalid block mode: {}", other)).into(),
                },
                other => return Err(format!("Invalid block mode: {}", other)).into(),
            },
            other => return Err(format!("Invalid cipher: AES-{}", other)).into(),
        },
        other => return Err(format!("Invalid cipher: {}", other)).into(),
    }

    unimplemented!()
}

#[derive(Clone, Copy, Debug)]
pub struct Encrypt;

impl Function for Encrypt {
    fn identifier(&self) -> &'static str {
        "encrypt"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "plaintext",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "algorithm",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "encrypt",
            source: r#"encrypt("secret data", "secret key")"#,
            result: Ok("5"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let plaintext = arguments.required("plaintext");
        let algorithm = arguments.required("algorithm");
        let key = arguments.required("key");

        Ok(Box::new(EncryptFn {
            plaintext,
            algorithm,
            key,
        }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let plaintext = args.required("plaintext");
        let algorithm = args.required("algorithm");
        let key = args.required("key");
        encrypt(plaintext, algorithm, key)
    }
}

#[derive(Debug, Clone)]
struct EncryptFn {
    plaintext: Box<dyn Expression>,
    algorithm: Box<dyn Expression>,
    key: Option<Box<dyn Expression>>,
}

impl Expression for EncryptFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        unimplemented!()
        // let value = self.value.resolve(ctx)?;
        //
        // strlen(value)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::integer().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        strlen => Strlen;

        string_value {
            args: func_args![value: value!("ñandú")],
            want: Ok(value!(5)),
            tdef: TypeDef::integer().infallible(),
        }
    ];
}
