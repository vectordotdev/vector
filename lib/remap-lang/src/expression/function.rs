use super::Error as E;
use crate::{
    expression, function::ArgumentList, state, Expr, Expression, Function as Fn, Object, Result,
    TypeDef, Value,
};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("undefined")]
    Undefined,

    #[error("invalid argument count (expected at most {0}, got {0})")]
    Arity(usize, usize),

    #[error(r#"unknown argument keyword "{0}""#)]
    Keyword(String),

    #[error(r#"missing required argument "{0}" (position {1})"#)]
    Required(String, usize),

    #[error(r#"unknown keyword "{0}""#)]
    Unknown(&'static str),

    #[error(r#"error for argument "{0}""#)]
    Argument(String, #[source] expression::argument::Error),
}

#[derive(Debug, Clone)]
pub struct Function {
    function: Box<dyn Expression>,

    // only used for `PartialEq` impl
    ident: &'static str,
}

impl PartialEq for Function {
    fn eq(&self, other: &Self) -> bool {
        self.ident == other.ident
    }
}

impl Function {
    pub fn new(
        ident: String,
        arguments: Vec<(Option<String>, Expr)>,
        definitions: &[Box<dyn Fn>],
    ) -> Result<Self> {
        let definition = definitions
            .iter()
            .find(|b| b.identifier() == ident)
            .ok_or_else(|| E::Function(ident.clone(), Error::Undefined))?;

        let ident = definition.identifier();
        let parameters = definition.parameters();

        // check function arity
        if arguments.len() > parameters.len() {
            return Err(E::Function(
                ident.to_owned(),
                Error::Arity(parameters.len(), arguments.len()),
            )
            .into());
        }

        // Keeps track of positional argument indices.
        //
        // Used to map a positional argument to its keyword. Keyword arguments
        // can be used in any order, and don't count towards the index of
        // positional arguments.
        let mut index = 0;
        let mut list = ArgumentList::default();

        for (keyword, argument) in arguments {
            let param = match &keyword {
                // positional argument
                None => {
                    index += 1;
                    parameters.get(index - 1)
                }

                // keyword argument
                Some(k) => parameters
                    .iter()
                    .enumerate()
                    .find(|(_, param)| param.keyword == k)
                    .map(|(pos, param)| {
                        if pos == index {
                            index += 1;
                        }

                        param
                    }),
            }
            .ok_or_else(|| {
                E::Function(
                    ident.to_owned(),
                    Error::Keyword(keyword.expect("arity checked")),
                )
            })?;

            let argument =
                expression::Argument::new(Box::new(argument), param.accepts, param.keyword, ident)
                    .into();

            list.insert(param.keyword, argument);
        }

        // check missing required arguments
        parameters
            .iter()
            .enumerate()
            .filter(|(_, p)| p.required)
            .filter(|(_, p)| !list.keywords().contains(&p.keyword))
            .map(|(i, p)| {
                Err(E::Function(ident.to_owned(), Error::Required(p.keyword.to_owned(), i)).into())
            })
            .collect::<Result<_>>()?;

        let function = definition.compile(list)?;
        Ok(Self { function, ident })
    }
}

impl Expression for Function {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        self.function.execute(state, object)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.function.type_def(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{expression::Noop, test_type_def, value::Kind};

    test_type_def![pass_through {
        expr: |_| {
            let function = Box::new(Noop);
            Function {
                function,
                ident: "foo",
            }
        },
        def: TypeDef {
            kind: Kind::Null,
            ..Default::default()
        },
    }];
}
