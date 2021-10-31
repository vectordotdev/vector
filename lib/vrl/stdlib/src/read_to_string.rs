use std::fs;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ReadToString;

impl Function for ReadToString {
    fn identifier(&self) -> &'static str {
        "read_to_string"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "path",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "path exists",
                source: r#"read_to_string("/path/to/file")"#,
                result: Ok("file contents.."),
            },
            Example {
                title: "path does not exists",
                source: r#"read_to_string("/path/to/non/existing/file")"#,
                result: Err("No such file or directory (os error 2"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let path = arguments.required("path");

        Ok(Box::new(ReadToStringFn { path }))
    }
}

#[derive(Debug, Clone)]
struct ReadToStringFn {
    path: Box<dyn Expression>,
}

impl Expression for ReadToStringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.path.resolve(ctx)?;
        let path = value.try_bytes_utf8_lossy()?;

        fs::read_to_string(path.as_ref())
            .map(Into::into)
            .map_err(|e| e.to_string().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod test {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    test_function![

        read_to_string => ReadToString;

        before_each => {
            let path = "/tmp/planet.txt";
            let data = "earth";

            let mut f = File::create(path).expect("Unable to create file");
            f.write_all(data.as_bytes()).expect("Unable to write data");
        }

        read_to_string_succ {
             args: func_args![path: "/tmp/planet.txt" ],
             want: Ok("earth"),
             tdef: TypeDef::new().fallible().bytes(),
         }
        read_to_string_err {
             args: func_args![path: "planet.txt" ],
             want: Err("No such file or directory (os error 2)"),
             tdef: TypeDef::new().fallible().bytes(),
         }
    ];
}
