use super::Error as E;
use crate::{Argument, ArgumentList, Expression, Function as Fn, Object, Result, State, Value};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("undefined")]
    Undefined,

    #[error("invalid argument count (expected at most {0}, got {0})")]
    Arity(usize, usize),

    #[error(r#"unknown argument keyword "{0}""#)]
    Keyword(String),

    #[error(r#"missing required argument "{0}" (position {1})"#)]
    Required(String, usize),
}

#[derive(Debug)]
pub(crate) struct Function {
    function: Box<dyn Expression>,
}

impl Function {
    pub(crate) fn new(
        ident: String,
        arguments: Vec<(Option<String>, Argument)>,
        definitions: &[Box<dyn Fn>],
    ) -> Result<Self> {
        let definition = definitions
            .iter()
            .find(|b| b.identifier() == ident)
            .ok_or_else(|| E::Function(ident.clone(), Error::Undefined))?;

        let parameters = definition.parameters();

        // check function arity
        if arguments.len() > parameters.len() {
            return Err(E::Function(
                ident.clone(),
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
                    ident.clone(),
                    Error::Keyword(keyword.expect("arity checked")),
                )
            })?;

            list.insert(param.keyword, argument);
        }

        // check missing required arguments
        parameters
            .iter()
            .enumerate()
            .filter(|(_, p)| p.required)
            .filter(|(_, p)| !list.keywords().contains(&p.keyword))
            .map(|(i, p)| {
                Err(E::Function(ident.clone(), Error::Required(p.keyword.to_owned(), i)).into())
            })
            .collect::<Result<_>>()?;

        let function = definition.compile(list)?;
        Ok(Self { function })
    }
}

impl Expression for Function {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        self.function.execute(state, object)
    }
}
