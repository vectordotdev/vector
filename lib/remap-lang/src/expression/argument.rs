use crate::{
    expression, expression::Error as E, state, value, Expr, Expression, Object, Result, TypeDef,
    Value,
};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(r#"invalid value type (got "{0}")"#)]
    Invalid(value::Kind),
}

#[derive(Clone)]
pub struct Argument {
    expression: Box<Expr>,
    ident: &'static str,
    keyword: &'static str,
    validator: fn(&Value) -> bool,

    // used for error messages
    function_ident: &'static str,
}

impl std::fmt::Debug for Argument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Argument")
            .field("expression", &self.expression)
            .field("ident", &self.ident)
            .field("keyword", &self.keyword)
            .field("validator", &"fn(&Value) -> bool".to_owned())
            .finish()
    }
}

impl PartialEq for Argument {
    fn eq(&self, other: &Self) -> bool {
        self.expression == other.expression
            && self.ident == other.ident
            && self.keyword == other.keyword
            && self.function_ident == other.function_ident
    }
}

impl Argument {
    pub fn new(
        expression: Box<Expr>,
        ident: &'static str,
        keyword: &'static str,
        validator: fn(&Value) -> bool,
        function_ident: &'static str,
    ) -> Self {
        Self {
            expression,
            ident,
            keyword,
            validator,
            function_ident,
        }
    }

    /// Consume the argument, returning its inner expression.
    ///
    /// Doing this will prevent the argument runtime check from running, but
    /// gives you access to the underlying concrete `Expr` type at compile-time.
    pub fn into_expr(self) -> Expr {
        *self.expression
    }
}

impl Expression for Argument {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.expression.execute(state, object)?;

        if !(self.validator)(&value) {
            return Err(E::Function(
                self.function_ident.to_owned(),
                expression::function::Error::Argument(
                    self.keyword.to_owned(),
                    Error::Invalid(value.kind()),
                ),
            )
            .into());
        }

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.expression.type_def(state)
    }
}
