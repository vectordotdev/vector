use crate::expression::{levenstein, Resolved};
use crate::parser::ast::Ident;
use crate::{Context, Expression, Span, State, TypeDef, Value};

use diagnostic::{DiagnosticError, Label};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    ident: Ident,
    value: Option<Value>,
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

        Ok(Self { ident, value })
    }

    pub fn ident(&self) -> &Ident {
        &self.ident
    }

    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
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
            .cloned()
            .unwrap_or(Value::Null))
    }

    fn type_def(&self, state: &State) -> TypeDef {
        state
            .variable(&self.ident)
            .cloned()
            .map(|d| d.type_def)
            .unwrap_or_else(|| TypeDef::new().null().infallible())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        _: &crate::state::Compiler,
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        let function = ctx.function();
        let variable_begin_block = ctx.context().append_basic_block(function, "variable_begin");
        ctx.builder()
            .build_unconditional_branch(variable_begin_block);
        ctx.builder().position_at_end(variable_begin_block);

        let fn_ident = "vrl_expression_variable_impl";
        let fn_impl = ctx
            .module()
            .get_function(fn_ident)
            .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
        let ident_name = format!("{:?}", self.ident);
        let ident_ref = ctx
            .into_const(self.ident.clone(), &ident_name)
            .as_pointer_value();
        ctx.builder().build_call(
            fn_impl,
            &[
                ctx.context_ref().into(),
                ctx.builder()
                    .build_bitcast(
                        ident_ref,
                        fn_impl
                            .get_nth_param(1)
                            .unwrap()
                            .get_type()
                            .into_pointer_type(),
                        "cast",
                    )
                    .into(),
                ctx.result_ref().into(),
            ],
            fn_ident,
        );
        Ok(())
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
