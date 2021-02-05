// commonly used modules
pub use compiler::{state, value, value::kind};

// pub use crate::{expression, function, state, value};

// commonly used top-level crate types
pub use compiler::{value::Kind, Context, Expression, Function, Resolved, Target, TypeDef, Value};

pub use diagnostic::DiagnosticError;

// pub use crate::{Error, Expr, Expression, Function, Object, Result, TypeDef, Value};

// commonly used expressions

// pub use compiler::expression::Resolved;

// commonly used function types

pub use compiler::function::{ArgumentList, Compiled, Parameter};

// commonly used macros

// pub use crate::generate_param_list;

// test helpers
// pub use crate::{array, bench_function, func_args, lit, map, test_function, test_type_def};
