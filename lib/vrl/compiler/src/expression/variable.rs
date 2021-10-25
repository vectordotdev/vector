use crate::expression::{levenstein, Resolved};
use crate::parser::ast::Ident;
use crate::{Context, Expression, EzValue, Span, State, TypeDef, Value};

use diagnostic::{DiagnosticError, Label};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    ident: Ident,
    value: Option<EzValue>,
}

impl Variable {
    pub(crate) fn new(span: Span, ident: Ident, state: &State) -> Result<Self, Error> {
        let value = match state.variable(&ident) {
            Some(variable) => variable.value.as_ref().cloned(),
            None => {
                let idents = state
                    .variable_idents()
                    .map(|s| s.to_owned())
                    .collect::<Vec<_>>();

                return Err(Error::undefined(ident, span, idents));
            }
        };

        Ok(Self {
            ident,
            value: value.map(|v| v.borrow().clone().into()),
        })
    }

    pub fn ident(&self) -> &Ident {
        &self.ident
    }

    pub fn value(&self) -> Option<Rc<RefCell<Value>>> {
        self.value
            .as_ref()
            .map(|v| Rc::new(RefCell::new(v.clone().into())))
    }

    pub fn noop(ident: Ident) -> Self {
        Self { ident, value: None }
    }
}

impl Expression for Variable {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        Ok(ctx
            .state()
            .variable(&self.ident)
            .unwrap_or_else(|| Rc::new(RefCell::new(Value::Null))))
    }

    fn type_def(&self, state: &State) -> TypeDef {
        state
            .variable(&self.ident)
            .cloned()
            .map(|d| d.type_def)
            .unwrap_or_else(|| TypeDef::new().null().infallible())
    }
}

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ident.fmt(f)
    }
}

#[derive(Debug)]
pub struct Error {
    variant: ErrorVariant,
    ident: Ident,
    span: Span,
}

impl Error {
    fn undefined(ident: Ident, span: Span, idents: Vec<Ident>) -> Self {
        Error {
            variant: ErrorVariant::Undefined { idents },
            ident,
            span,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ErrorVariant {
    #[error("call to undefined variable")]
    Undefined { idents: Vec<Ident> },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.variant)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.variant)
    }
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use ErrorVariant::*;

        match &self.variant {
            Undefined { .. } => 701,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::*;

        match &self.variant {
            Undefined { idents } => {
                let mut vec = vec![Label::primary("undefined variable", self.span)];
                let ident_chars = self.ident.as_ref().chars().collect::<Vec<_>>();

                let mut builtin = vec![Ident::new("null"), Ident::new("true"), Ident::new("false")];
                let mut idents = idents.clone();

                idents.append(&mut builtin);

                if let Some((idx, _)) = idents
                    .iter()
                    .map(|possible| {
                        let possible_chars = possible.chars().collect::<Vec<_>>();
                        levenstein::distance(&ident_chars, &possible_chars)
                    })
                    .enumerate()
                    .min_by_key(|(_, score)| *score)
                {
                    {
                        let guessed = &idents[idx];
                        vec.push(Label::context(
                            format!(r#"did you mean "{}"?"#, guessed),
                            self.span,
                        ));
                    }
                }

                vec
            }
        }
    }
}
