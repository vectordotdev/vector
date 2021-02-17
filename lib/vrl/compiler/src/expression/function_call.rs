use crate::expression::{FunctionArgument, Noop};
use crate::function::ArgumentList;
use crate::parser::{Ident, Node};
use crate::{value::Kind, Context, Expression, Function, Resolved, Span, State, TypeDef};
use diagnostic::{DiagnosticError, Label, Note};
use std::fmt;

#[derive(Clone)]
pub struct FunctionCall {
    abort_on_error: bool,
    expr: Box<dyn Expression>,

    // used for enhancing runtime error messages (using abort-instruction).
    //
    // TODO: have span store line/col details to further improve this.
    span: Span,

    // Used for pretty-printing function call.
    //
    // This allows us to keep the arguments non-cloneable.
    arguments_fmt: Vec<String>,
    arguments_dbg: Vec<String>,

    // used for equality check
    ident: &'static str,
}

impl FunctionCall {
    pub fn new(
        call_span: Span,
        ident: Node<Ident>,
        abort_on_error: bool,
        arguments: Vec<Node<FunctionArgument>>,
        funcs: &[Box<dyn Function>],
        state: &State,
    ) -> Result<Self, Error> {
        let (ident_span, ident) = ident.take();

        // Check if function exists.
        let function = match funcs.iter().find(|f| f.identifier() == ident.as_ref()) {
            Some(function) => function,
            None => {
                let idents = funcs
                    .iter()
                    .map(|func| func.identifier())
                    .collect::<Vec<_>>();

                return Err(Error::Undefined {
                    ident_span,
                    ident: ident.clone(),
                    idents,
                });
            }
        };

        // Check function arity.
        if arguments.len() > function.parameters().len() {
            let arguments_span = {
                let start = arguments.first().unwrap().span().start();
                let end = arguments.last().unwrap().span().end();

                Span::new(start, end)
            };

            return Err(Error::ArityMismatch {
                arguments_span,
                max: function.parameters().len(),
            });
        }

        // Keeps track of positional argument indices.
        //
        // Used to map a positional argument to its keyword. Keyword arguments
        // can be used in any order, and don't count towards the index of
        // positional arguments.
        let mut index = 0;
        let mut list = ArgumentList::default();

        let arguments_fmt = arguments
            .iter()
            .map(|arg| arg.to_string())
            .collect::<Vec<_>>();

        let arguments_dbg = arguments
            .iter()
            .map(|arg| format!("{:?}", arg))
            .collect::<Vec<_>>();

        for node in arguments {
            let (argument_span, argument) = node.take();

            let parameter = match argument.keyword() {
                // positional argument
                None => {
                    index += 1;
                    function.parameters().get(index - 1)
                }

                // keyword argument
                Some(k) => function
                    .parameters()
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
            .ok_or_else(|| Error::UnknownKeyword {
                keyword_span: argument.keyword_span().expect("exists"),
                ident_span,
                keywords: function.parameters().iter().map(|p| p.keyword).collect(),
            })?;

            // Check if the argument is of the expected type.
            let expr_kind = argument.type_def(state).kind();
            if !parameter.kind().contains(expr_kind) {
                return Err(Error::InvalidArgumentKind {
                    keyword: parameter.keyword,
                    got: expr_kind,
                    expected: parameter.kind(),
                    expr_span: argument.span(),
                    argument_span,
                });
            }

            // Check if the argument is infallible.
            if argument.type_def(state).is_fallible() {
                return Err(Error::FallibleArgument {
                    expr_span: argument.span(),
                });
            }

            list.insert(parameter.keyword, argument.into_inner());
        }

        // Check missing required arguments.
        function
            .parameters()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.required)
            .filter(|(_, p)| !list.keywords().contains(&p.keyword))
            .try_for_each(|(i, p)| -> Result<_, _> {
                Err(Error::RequiredArgument {
                    call_span,
                    keyword: p.keyword,
                    position: i,
                })
            })?;

        let expr = function
            .compile(list)
            .map_err(|error| Error::Compilation { call_span, error })?;

        // Asking for an infallible function to abort on error makes no sense.
        // We consider this an error at compile-time, because it makes the
        // resulting program incorrectly convey this function call might fail.
        let type_def = expr.type_def(state);

        if abort_on_error && !type_def.is_fallible() {
            return Err(Error::AbortInfallible {
                ident_span,
                abort_span: Span::new(ident_span.end(), ident_span.end() + 1),
            });
        }

        Ok(Self {
            span: call_span,
            abort_on_error,
            expr,
            arguments_fmt,
            arguments_dbg,
            ident: function.identifier(),
        })
    }

    pub fn noop() -> Self {
        let expr = Box::new(Noop) as _;

        Self {
            span: Span::default(),
            abort_on_error: false,
            expr,
            arguments_fmt: vec![],
            arguments_dbg: vec![],
            ident: "noop",
        }
    }
}

impl Expression for FunctionCall {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.expr.resolve(ctx).map_err(|mut err| {
            err.message = format!(
                r#"function call error for "{}" at ({}:{}): {}"#,
                self.ident,
                self.span.start(),
                self.span.end(),
                err.message
            );

            err
        })
    }

    fn type_def(&self, state: &State) -> TypeDef {
        let mut type_def = self.expr.type_def(state);

        if self.abort_on_error {
            type_def.fallible = false;
        }

        type_def
    }
}

impl fmt::Display for FunctionCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ident.fmt(f)?;
        f.write_str("(")?;

        let mut iter = self.arguments_fmt.iter().peekable();
        while let Some(arg) = iter.next() {
            f.write_str(arg)?;

            if iter.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str(")")
    }
}

impl fmt::Debug for FunctionCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FunctionCall(")?;
        self.ident.fmt(f)?;

        f.write_str("(")?;

        let mut iter = self.arguments_dbg.iter().peekable();
        while let Some(arg) = iter.next() {
            f.write_str(arg)?;

            if iter.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str("))")
    }
}

impl PartialEq for FunctionCall {
    fn eq(&self, other: &Self) -> bool {
        self.ident == other.ident
    }
}

// -----------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("call to undefined function")]
    Undefined {
        ident_span: Span,
        ident: Ident,
        idents: Vec<&'static str>,
    },

    #[error("function argument arity mismatch")]
    ArityMismatch { arguments_span: Span, max: usize },

    #[error("unknown function argument keyword")]
    UnknownKeyword {
        keyword_span: Span,
        ident_span: Span,
        keywords: Vec<&'static str>,
    },

    #[error("function argument missing")]
    RequiredArgument {
        call_span: Span,
        keyword: &'static str,
        position: usize,
    },

    #[error("function compilation error")]
    Compilation {
        call_span: Span,
        error: Box<dyn DiagnosticError>,
    },

    #[error("cannot abort function that never fails")]
    AbortInfallible { ident_span: Span, abort_span: Span },

    #[error("invalid argument type")]
    InvalidArgumentKind {
        keyword: &'static str,
        got: Kind,
        expected: Kind,
        expr_span: Span,
        argument_span: Span,
    },

    #[error("fallible argument")]
    FallibleArgument { expr_span: Span },
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            Undefined { .. } => 105,
            ArityMismatch { .. } => 106,
            UnknownKeyword { .. } => 108,
            Compilation { .. } => 610,
            RequiredArgument { .. } => 107,
            AbortInfallible { .. } => 620,
            InvalidArgumentKind { .. } => 110,
            FallibleArgument { .. } => 630,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::*;

        match self {
            Undefined {
                ident_span,
                ident,
                idents,
            } => {
                let mut vec = vec![Label::primary("undefined function", ident_span)];

                let mut corpus = ngrammatic::CorpusBuilder::new()
                    .arity(2)
                    .pad_full(ngrammatic::Pad::Auto)
                    .finish();

                for func in idents {
                    corpus.add_text(func);
                }

                if let Some(guess) = corpus.search(ident.as_ref(), 0.25).first() {
                    vec.push(Label::context(
                        format!(r#"did you mean "{}"?"#, guess.text),
                        ident_span,
                    ));
                }

                vec
            }

            ArityMismatch {
                arguments_span,
                max,
            } => {
                let arg = if *max == 1 { "argument" } else { "arguments" };

                vec![
                    Label::primary("too many function arguments", arguments_span),
                    Label::context(
                        format!("this function takes a maximum of {} {}", max, arg),
                        arguments_span,
                    ),
                ]
            }

            UnknownKeyword {
                keyword_span,
                ident_span,
                keywords,
            } => vec![
                Label::primary("unknown keyword", keyword_span),
                Label::context(
                    format!(
                        "this function accepts the following keywords: {}",
                        keywords
                            .iter()
                            .map(|k| format!(r#""{}""#, k))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    ident_span,
                ),
            ],

            Compilation { call_span, error } => error
                .labels()
                .into_iter()
                .map(|mut label| {
                    label.span = *call_span;
                    label
                })
                .collect(),

            RequiredArgument {
                call_span,
                keyword,
                position,
            } => {
                vec![Label::primary(
                    format!(
                        r#"required argument missing: "{}" (position {})"#,
                        keyword, position
                    ),
                    call_span,
                )]
            }

            AbortInfallible {
                ident_span,
                abort_span,
            } => {
                vec![
                    Label::primary("this function cannot fail", ident_span),
                    Label::context("remove this abort-instruction", abort_span),
                ]
            }

            InvalidArgumentKind {
                expr_span,
                argument_span,
                keyword,
                got,
                expected,
            } => {
                // TODO: extract this out into a helper
                let kind_str = |kind: &Kind| {
                    if kind.is_any() {
                        kind.to_string()
                    } else if !kind.is_many() {
                        format!(r#"the exact type {}"#, kind)
                    } else {
                        format!("one of {}", kind)
                    }
                };

                vec![
                    Label::primary(
                        format!("this expression resolves to {}", kind_str(got)),
                        expr_span,
                    ),
                    Label::context(
                        format!(
                            r#"but the parameter "{}" expects {}"#,
                            keyword,
                            kind_str(expected)
                        ),
                        argument_span,
                    ),
                ]
            }

            FallibleArgument { expr_span } => vec![
                Label::primary("this expression can fail", expr_span),
                Label::context(
                    "handle the error before passing it in as an argument",
                    expr_span,
                ),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use Error::*;

        match self {
            AbortInfallible { .. } | FallibleArgument { .. } => vec![Note::SeeErrorDocs],
            _ => vec![],
        }
    }
}
