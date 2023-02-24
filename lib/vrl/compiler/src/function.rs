pub mod closure;

use diagnostic::{DiagnosticMessage, Label, Note};
use lookup::OwnedTargetPath;
use parser::ast::Ident;
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
};
use value::{kind::Collection, Value};

use crate::{
    expression::{container::Variant, Block, Container, Expr, Expression},
    state::TypeState,
    value::{kind, Kind},
    CompileConfig, Span, TypeDef,
};

pub type Compiled = Result<Box<dyn Expression>, Box<dyn DiagnosticMessage>>;
pub type CompiledArgument =
    Result<Option<Box<dyn std::any::Any + Send + Sync>>, Box<dyn DiagnosticMessage>>;

pub trait Function: Send + Sync + fmt::Debug {
    /// The identifier by which the function can be called.
    fn identifier(&self) -> &'static str;

    /// A brief single-line description explaining what this function does.
    fn summary(&self) -> &'static str {
        "TODO"
    }

    /// A more elaborate multi-paragraph description on how to use the function.
    fn usage(&self) -> &'static str {
        "TODO"
    }

    /// One or more examples demonstrating usage of the function in VRL source
    /// code.
    fn examples(&self) -> &'static [Example];

    /// Compile a [`Function`] into a type that can be resolved to an
    /// [`Expression`].
    ///
    /// This function is called at compile-time for any `Function` used in the
    /// program.
    ///
    /// At runtime, the `Expression` returned by this function is executed and
    /// resolved to its final [`Value`].
    fn compile(
        &self,
        state: &TypeState,
        ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled;

    /// An optional list of parameters the function accepts.
    ///
    /// This list is used at compile-time to check function arity, keyword names
    /// and argument type definition.
    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    /// An optional closure definition for the function.
    ///
    /// This returns `None` by default, indicating the function doesn't accept
    /// a closure.
    fn closure(&self) -> Option<closure::Definition> {
        None
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Example {
    pub title: &'static str,
    pub source: &'static str,
    pub result: Result<&'static str, &'static str>,
}

pub struct FunctionCompileContext {
    span: Span,
    config: CompileConfig,
}

impl FunctionCompileContext {
    #[must_use]
    pub fn new(span: Span, config: CompileConfig) -> Self {
        Self { span, config }
    }

    /// Span information for the function call.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }

    /// Get an immutable reference to a stored external context, if one exists.
    #[must_use]
    pub fn get_external_context<T: 'static>(&self) -> Option<&T> {
        self.config.get_custom()
    }

    /// Get a mutable reference to a stored external context, if one exists.
    pub fn get_external_context_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.config.get_custom_mut()
    }

    #[must_use]
    pub fn is_read_only_path(&self, path: &OwnedTargetPath) -> bool {
        self.config.is_read_only_path(path)
    }

    /// Consume the `FunctionCompileContext`, returning the (potentially mutated) `AnyMap`.
    #[must_use]
    pub fn into_config(self) -> CompileConfig {
        self.config
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// The keyword of the parameter.
    ///
    /// Arguments can be passed in both using the keyword, or as a positional
    /// argument.
    pub keyword: &'static str,

    /// The type kind(s) this parameter expects to receive.
    ///
    /// If an invalid kind is provided, the compiler will return a compile-time
    /// error.
    pub kind: u16,

    /// Whether or not this is a required parameter.
    ///
    /// If it isn't, the function can be called without errors, even if the
    /// argument matching this parameter is missing.
    pub required: bool,
}

impl Parameter {
    #[allow(arithmetic_overflow)]
    #[must_use]
    pub fn kind(&self) -> Kind {
        let mut kind = Kind::never();

        let n = self.kind;

        if (n & kind::BYTES) == kind::BYTES {
            kind.add_bytes();
        }

        if (n & kind::INTEGER) == kind::INTEGER {
            kind.add_integer();
        }

        if (n & kind::FLOAT) == kind::FLOAT {
            kind.add_float();
        }

        if (n & kind::BOOLEAN) == kind::BOOLEAN {
            kind.add_boolean();
        }

        if (n & kind::OBJECT) == kind::OBJECT {
            kind.add_object(Collection::any());
        }

        if (n & kind::ARRAY) == kind::ARRAY {
            kind.add_array(Collection::any());
        }

        if (n & kind::TIMESTAMP) == kind::TIMESTAMP {
            kind.add_timestamp();
        }

        if (n & kind::REGEX) == kind::REGEX {
            kind.add_regex();
        }

        if (n & kind::NULL) == kind::NULL {
            kind.add_null();
        }

        if (n & kind::UNDEFINED) == kind::UNDEFINED {
            kind.add_undefined();
        }

        kind
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct ArgumentList {
    pub(crate) arguments: HashMap<&'static str, Expr>,

    /// A closure argument differs from regular arguments, in that it isn't an
    /// expression by itself, and it also isn't tied to a parameter string in
    /// the function call.
    ///
    /// We do still want to store the closure in the argument list, to allow
    /// function implementors access to the closure through `Function::compile`.
    closure: Option<FunctionClosure>,
}

impl ArgumentList {
    #[must_use]
    pub fn optional(&self, keyword: &'static str) -> Option<Box<dyn Expression>> {
        self.optional_expr(keyword).map(|v| Box::new(v) as _)
    }

    #[must_use]
    pub fn required(&self, keyword: &'static str) -> Box<dyn Expression> {
        Box::new(self.required_expr(keyword)) as _
    }

    #[cfg(feature = "expr-literal")]
    pub fn optional_literal(
        &self,
        keyword: &'static str,
    ) -> Result<Option<crate::expression::Literal>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                Expr::Literal(literal) => Ok(literal),
                Expr::Variable(var) if var.value().is_some() => {
                    match var.value().unwrap().clone().into() {
                        Expr::Literal(literal) => Ok(literal),
                        expr => Err(Error::UnexpectedExpression {
                            keyword,
                            expected: "literal",
                            expr,
                        }),
                    }
                }
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "literal",
                    expr,
                }),
            })
            .transpose()
    }

    #[cfg(not(feature = "expr-literal"))]
    pub fn optional_literal(
        &self,
        _: &'static str,
    ) -> Result<Option<crate::expression::Noop>, Error> {
        Ok(Some(crate::expression::Noop))
    }

    /// Returns the argument if it is a literal, an object or an array.
    pub fn optional_value(&self, keyword: &'static str) -> Result<Option<Value>, Error> {
        self.optional_expr(keyword)
            .map(|expr| {
                expr.try_into().map_err(|err| Error::UnexpectedExpression {
                    keyword,
                    expected: "literal",
                    expr: err,
                })
            })
            .transpose()
    }

    #[cfg(feature = "expr-literal")]
    pub fn required_literal(
        &self,
        keyword: &'static str,
    ) -> Result<crate::expression::Literal, Error> {
        Ok(required(self.optional_literal(keyword)?))
    }

    #[cfg(not(feature = "expr-literal"))]
    pub fn required_literal(
        &mut self,
        keyword: &'static str,
    ) -> Result<crate::expression::Noop, Error> {
        Ok(required(self.optional_literal(keyword)?))
    }

    pub fn optional_enum(
        &self,
        keyword: &'static str,
        variants: &[Value],
    ) -> Result<Option<Value>, Error> {
        self.optional_literal(keyword)?
            .and_then(|literal| literal.as_value())
            .map(|value| {
                variants
                    .iter()
                    .find(|v| *v == &value)
                    .cloned()
                    .ok_or(Error::InvalidEnumVariant {
                        keyword,
                        value,
                        variants: variants.to_vec(),
                    })
            })
            .transpose()
    }

    pub fn required_enum(&self, keyword: &'static str, variants: &[Value]) -> Result<Value, Error> {
        Ok(required(self.optional_enum(keyword, variants)?))
    }

    #[cfg(feature = "expr-query")]
    pub fn optional_query(
        &self,
        keyword: &'static str,
    ) -> Result<Option<crate::expression::Query>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                Expr::Query(query) => Ok(query),
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "query",
                    expr,
                }),
            })
            .transpose()
    }

    #[cfg(feature = "expr-query")]
    pub fn required_query(&self, keyword: &'static str) -> Result<crate::expression::Query, Error> {
        Ok(required(self.optional_query(keyword)?))
    }

    pub fn optional_regex(&self, keyword: &'static str) -> Result<Option<regex::Regex>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                #[cfg(feature = "expr-literal")]
                Expr::Literal(crate::expression::Literal::Regex(regex)) => Ok((*regex).clone()),
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "regex",
                    expr,
                }),
            })
            .transpose()
    }

    pub fn required_regex(&self, keyword: &'static str) -> Result<regex::Regex, Error> {
        Ok(required(self.optional_regex(keyword)?))
    }

    pub fn optional_object(
        &self,
        keyword: &'static str,
    ) -> Result<Option<BTreeMap<String, Expr>>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                Expr::Container(Container {
                    variant: Variant::Object(object),
                }) => Ok((*object).clone()),
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "object",
                    expr,
                }),
            })
            .transpose()
    }

    pub fn required_object(&self, keyword: &'static str) -> Result<BTreeMap<String, Expr>, Error> {
        Ok(required(self.optional_object(keyword)?))
    }

    pub fn optional_array(&self, keyword: &'static str) -> Result<Option<Vec<Expr>>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                Expr::Container(Container {
                    variant: Variant::Array(array),
                }) => Ok((*array).clone()),
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "array",
                    expr,
                }),
            })
            .transpose()
    }

    pub fn required_array(&self, keyword: &'static str) -> Result<Vec<Expr>, Error> {
        Ok(required(self.optional_array(keyword)?))
    }

    #[must_use]
    pub fn optional_closure(&self) -> Option<&FunctionClosure> {
        self.closure.as_ref()
    }

    pub fn required_closure(&self) -> Result<FunctionClosure, Error> {
        self.optional_closure()
            .cloned()
            .ok_or(Error::ExpectedFunctionClosure)
    }

    #[cfg(feature = "expr-function_call")]
    pub(crate) fn keywords(&self) -> Vec<&'static str> {
        self.arguments.keys().copied().collect::<Vec<_>>()
    }

    #[cfg(feature = "expr-function_call")]
    pub(crate) fn insert(&mut self, k: &'static str, v: Expr) {
        self.arguments.insert(k, v);
    }

    #[cfg(feature = "expr-function_call")]
    pub(crate) fn set_closure(&mut self, closure: FunctionClosure) {
        self.closure = Some(closure);
    }

    pub(crate) fn optional_expr(&self, keyword: &'static str) -> Option<Expr> {
        self.arguments.get(keyword).cloned()
    }

    #[must_use]
    pub fn required_expr(&self, keyword: &'static str) -> Expr {
        required(self.optional_expr(keyword))
    }
}

fn required<T>(argument: Option<T>) -> T {
    argument.expect("invalid function signature")
}

#[cfg(any(test, feature = "test"))]
mod test_impls {
    use super::*;
    use crate::expression::FunctionArgument;
    use crate::parser::Node;

    impl From<HashMap<&'static str, Value>> for ArgumentList {
        fn from(map: HashMap<&'static str, Value>) -> Self {
            Self {
                arguments: map
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<HashMap<_, _>>(),
                closure: None,
            }
        }
    }

    impl From<ArgumentList> for Vec<(&'static str, Option<FunctionArgument>)> {
        fn from(args: ArgumentList) -> Self {
            args.arguments
                .iter()
                .map(|(key, expr)| {
                    (
                        *key,
                        Some(FunctionArgument::new(
                            None,
                            Node::new(Span::default(), expr.clone()),
                        )),
                    )
                })
                .collect()
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionClosure {
    pub variables: Vec<Ident>,
    pub block: Block,
    pub block_type_def: TypeDef,
}

impl FunctionClosure {
    #[must_use]
    pub fn new<T: Into<Ident>>(variables: Vec<T>, block: Block, block_type_def: TypeDef) -> Self {
        Self {
            variables: variables.into_iter().map(Into::into).collect(),
            block,
            block_type_def,
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("unexpected expression type")]
    UnexpectedExpression {
        keyword: &'static str,
        expected: &'static str,
        expr: Expr,
    },

    #[error(r#"invalid enum variant""#)]
    InvalidEnumVariant {
        keyword: &'static str,
        value: Value,
        variants: Vec<Value>,
    },

    #[error("this argument must be a static expression")]
    ExpectedStaticExpression { keyword: &'static str, expr: Expr },

    #[error(r#"invalid argument"#)]
    InvalidArgument {
        keyword: &'static str,
        value: Value,
        error: &'static str,
    },

    #[error(r#"missing function closure"#)]
    ExpectedFunctionClosure,

    #[error(r#"mutation of read-only value"#)]
    ReadOnlyMutation { context: String },
}

impl diagnostic::DiagnosticMessage for Error {
    fn code(&self) -> usize {
        use Error::{
            ExpectedFunctionClosure, ExpectedStaticExpression, InvalidArgument, InvalidEnumVariant,
            ReadOnlyMutation, UnexpectedExpression,
        };

        match self {
            UnexpectedExpression { .. } => 400,
            InvalidEnumVariant { .. } => 401,
            ExpectedStaticExpression { .. } => 402,
            InvalidArgument { .. } => 403,
            ExpectedFunctionClosure => 420,
            ReadOnlyMutation { .. } => 315,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::{
            ExpectedFunctionClosure, ExpectedStaticExpression, InvalidArgument, InvalidEnumVariant,
            ReadOnlyMutation, UnexpectedExpression,
        };

        match self {
            UnexpectedExpression {
                keyword,
                expected,
                expr,
            } => vec![
                Label::primary(
                    format!(r#"unexpected expression for argument "{keyword}""#),
                    Span::default(),
                ),
                Label::context(format!("received: {}", expr.as_str()), Span::default()),
                Label::context(format!("expected: {expected}"), Span::default()),
            ],

            InvalidEnumVariant {
                keyword,
                value,
                variants,
            } => vec![
                Label::primary(
                    format!(r#"invalid enum variant for argument "{keyword}""#),
                    Span::default(),
                ),
                Label::context(format!("received: {value}"), Span::default()),
                Label::context(
                    format!(
                        "expected one of: {}",
                        variants
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    Span::default(),
                ),
            ],

            ExpectedStaticExpression { keyword, expr } => vec![
                Label::primary(
                    format!(r#"expected static expression for argument "{keyword}""#),
                    Span::default(),
                ),
                Label::context(format!("received: {}", expr.as_str()), Span::default()),
            ],

            InvalidArgument {
                keyword,
                value,
                error,
            } => vec![
                Label::primary(format!(r#"invalid argument "{keyword}""#), Span::default()),
                Label::context(format!("received: {value}"), Span::default()),
                Label::context(format!("error: {error}"), Span::default()),
            ],

            ExpectedFunctionClosure => vec![],
            ReadOnlyMutation { context } => vec![
                Label::primary(r#"mutation of read-only value"#, Span::default()),
                Label::context(context, Span::default()),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        vec![Note::SeeCodeDocs(self.code())]
    }
}

impl From<Error> for Box<dyn diagnostic::DiagnosticMessage> {
    fn from(error: Error) -> Self {
        Box::new(error) as _
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_kind() {
        struct TestCase {
            parameter_kind: u16,
            kind: Kind,
        }

        for (
            title,
            TestCase {
                parameter_kind,
                kind,
            },
        ) in HashMap::from([
            (
                "bytes",
                TestCase {
                    parameter_kind: kind::BYTES,
                    kind: Kind::bytes(),
                },
            ),
            (
                "integer",
                TestCase {
                    parameter_kind: kind::INTEGER,
                    kind: Kind::integer(),
                },
            ),
        ]) {
            let parameter = Parameter {
                keyword: "",
                kind: parameter_kind,
                required: false,
            };

            assert_eq!(parameter.kind(), kind, "{title}");
        }
    }
}
