pub mod closure;

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
};

use anymap::AnyMap;
use diagnostic::{DiagnosticError, Label, Note};
use value::kind::Collection;

use crate::{
    expression::{
        container::Variant, Container, Expr, Expression, FunctionArgument, FunctionClosure,
        Literal, Query,
    },
    parser::Node,
    state::{ExternalEnv, LocalEnv},
    value::{kind, Kind},
    vm::VmArgumentList,
    Context, ExpressionError, Span, Value,
};

pub type Compiled = Result<Box<dyn Expression>, Box<dyn DiagnosticError>>;
pub type CompiledArgument =
    Result<Option<Box<dyn std::any::Any + Send + Sync>>, Box<dyn DiagnosticError>>;

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
    // fn examples(&self) -> &'static [Example] {
    //     &[/* ODO */]
    // }

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
        state: (&mut LocalEnv, &mut ExternalEnv),
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

    /// Implement this function if you need to manipulate and store any function parameters
    /// at compile time.
    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        _ctx: &mut FunctionCompileContext,
        _name: &str,
        _expr: Option<&Expr>,
    ) -> Result<Option<Box<dyn std::any::Any + Send + Sync>>, Box<dyn DiagnosticError>> {
        Ok(None)
    }

    /// This function is called by the VM.
    fn call_by_vm(
        &self,
        _ctx: &mut Context,
        _args: &mut VmArgumentList,
    ) -> Result<Value, ExpressionError>;

    /// An optional closure definition for the function.
    ///
    /// This function returns `None` by default, indicating the function doesn't
    /// accept a closure.
    fn closure(&self) -> Option<closure::Definition> {
        None
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Example {
    pub title: &'static str,
    pub source: &'static str,
    pub result: Result<&'static str, &'static str>,
}

#[derive(Debug)]
pub struct FunctionCompileContext {
    span: Span,
    external_context: AnyMap,
}

impl FunctionCompileContext {
    pub fn new(span: Span) -> Self {
        Self {
            span,
            external_context: AnyMap::new(),
        }
    }

    /// Add an external context to the compile context.
    pub fn with_external_context(mut self, context: AnyMap) -> Self {
        self.external_context = context;
        self
    }

    /// Span information for the function call.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Get an immutable reference to a stored external context, if one exists.
    pub fn get_external_context<T: 'static>(&self) -> Option<&T> {
        self.external_context.get::<T>()
    }

    /// Get a mutable reference to a stored external context, if one exists.
    pub fn get_external_context_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.external_context.get_mut::<T>()
    }

    /// Consume the `FunctionCompileContext`, returning the (potentially mutated) `AnyMap`.
    pub fn into_external_context(self) -> AnyMap {
        self.external_context
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq)]
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
    pub fn kind(&self) -> Kind {
        let mut kind = Kind::empty();

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

        kind
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct ArgumentList {
    pub(crate) arguments: HashMap<&'static str, Expr>,
    closure: Option<FunctionClosure>,
}

impl ArgumentList {
    pub fn optional(&mut self, keyword: &'static str) -> Option<Box<dyn Expression>> {
        self.optional_expr(keyword).map(|v| Box::new(v) as _)
    }

    pub fn required(&mut self, keyword: &'static str) -> Box<dyn Expression> {
        Box::new(self.required_expr(keyword)) as _
    }

    pub fn optional_literal(&mut self, keyword: &'static str) -> Result<Option<Literal>, Error> {
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

    /// Returns the argument if it is a literal, an object or an array.
    pub fn optional_value(&mut self, keyword: &'static str) -> Result<Option<Value>, Error> {
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

    pub fn required_literal(&mut self, keyword: &'static str) -> Result<Literal, Error> {
        Ok(required(self.optional_literal(keyword)?))
    }

    pub fn optional_enum(
        &mut self,
        keyword: &'static str,
        variants: &[Value],
    ) -> Result<Option<Value>, Error> {
        self.optional_literal(keyword)?
            .map(|literal| literal.to_value())
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

    pub fn required_enum(
        &mut self,
        keyword: &'static str,
        variants: &[Value],
    ) -> Result<Value, Error> {
        Ok(required(self.optional_enum(keyword, variants)?))
    }

    pub fn optional_query(&mut self, keyword: &'static str) -> Result<Option<Query>, Error> {
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

    pub fn required_query(&mut self, keyword: &'static str) -> Result<Query, Error> {
        Ok(required(self.optional_query(keyword)?))
    }

    pub fn optional_regex(&mut self, keyword: &'static str) -> Result<Option<regex::Regex>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                Expr::Literal(Literal::Regex(regex)) => Ok((*regex).clone()),
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "regex",
                    expr,
                }),
            })
            .transpose()
    }

    pub fn required_regex(&mut self, keyword: &'static str) -> Result<regex::Regex, Error> {
        Ok(required(self.optional_regex(keyword)?))
    }

    pub fn optional_object(
        &mut self,
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

    pub fn required_object(
        &mut self,
        keyword: &'static str,
    ) -> Result<BTreeMap<String, Expr>, Error> {
        Ok(required(self.optional_object(keyword)?))
    }

    pub fn optional_array(&mut self, keyword: &'static str) -> Result<Option<Vec<Expr>>, Error> {
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

    pub fn required_array(&mut self, keyword: &'static str) -> Result<Vec<Expr>, Error> {
        Ok(required(self.optional_array(keyword)?))
    }

    pub fn optional_closure(&self) -> Option<&FunctionClosure> {
        self.closure.as_ref()
    }

    pub fn required_closure(&self) -> Result<FunctionClosure, Error> {
        self.optional_closure()
            .cloned()
            .ok_or(Error::ExpectedFunctionClosure)
    }

    pub(crate) fn keywords(&self) -> Vec<&'static str> {
        self.arguments.keys().copied().collect::<Vec<_>>()
    }

    pub(crate) fn insert(&mut self, k: &'static str, v: Expr) {
        self.arguments.insert(k, v);
    }

    pub(crate) fn set_closure(&mut self, closure: FunctionClosure) {
        self.closure = Some(closure);
    }

    fn optional_expr(&mut self, keyword: &'static str) -> Option<Expr> {
        self.arguments.remove(keyword)
    }

    fn required_expr(&mut self, keyword: &'static str) -> Expr {
        required(self.optional_expr(keyword))
    }
}

fn required<T>(argument: Option<T>) -> T {
    argument.expect("invalid function signature")
}

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

impl From<Vec<Node<FunctionArgument>>> for ArgumentList {
    fn from(arguments: Vec<Node<FunctionArgument>>) -> Self {
        let arguments = arguments
            .into_iter()
            .map(|arg| {
                let arg = arg.into_inner();
                // TODO: find a better API design that doesn't require unwrapping.
                let key = arg.parameter().expect("exists").keyword;
                let expr = arg.into_inner();

                (key, expr)
            })
            .collect::<HashMap<_, _>>();

        Self {
            arguments,
            ..Default::default()
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
}

impl diagnostic::DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            UnexpectedExpression { .. } => 400,
            InvalidEnumVariant { .. } => 401,
            ExpectedStaticExpression { .. } => 402,
            InvalidArgument { .. } => 403,
            ExpectedFunctionClosure => 420,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::*;

        match self {
            UnexpectedExpression {
                keyword,
                expected,
                expr,
            } => vec![
                Label::primary(
                    format!(r#"unexpected expression for argument "{}""#, keyword),
                    Span::default(),
                ),
                Label::context(format!("received: {}", expr.as_str()), Span::default()),
                Label::context(format!("expected: {}", expected), Span::default()),
            ],

            InvalidEnumVariant {
                keyword,
                value,
                variants,
            } => vec![
                Label::primary(
                    format!(r#"invalid enum variant for argument "{}""#, keyword),
                    Span::default(),
                ),
                Label::context(format!("received: {}", value), Span::default()),
                Label::context(
                    format!(
                        "expected one of: {}",
                        variants
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    Span::default(),
                ),
            ],

            ExpectedStaticExpression { keyword, expr } => vec![
                Label::primary(
                    format!(r#"expected static expression for argument "{}""#, keyword),
                    Span::default(),
                ),
                Label::context(format!("received: {}", expr.as_str()), Span::default()),
            ],

            InvalidArgument {
                keyword,
                value,
                error,
            } => vec![
                Label::primary(
                    format!(r#"invalid argument "{}""#, keyword),
                    Span::default(),
                ),
                Label::context(format!("received: {}", value), Span::default()),
                Label::context(format!("error: {}", error), Span::default()),
            ],

            ExpectedFunctionClosure => vec![],
        }
    }

    fn notes(&self) -> Vec<Note> {
        vec![Note::SeeCodeDocs(self.code())]
    }
}

impl From<Error> for Box<dyn diagnostic::DiagnosticError> {
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

            assert_eq!(parameter.kind(), kind, "{}", title);
        }
    }
}
