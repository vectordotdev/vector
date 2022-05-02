use core::Target;

use lookup::LookupBuf;
use parser::ast::Opcode;
use value::Value;

use crate::{
    expression::{self, assignment, Expr},
    state::{ExternalEnv, LocalEnv},
    vm, Context, Expression,
};

#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) runner: Runner,
    pub(crate) info: ProgramInfo,

    /// A copy of the local environment at program compilation.
    ///
    /// Can be used to instantiate a new program with the same local state as
    /// the previous program.
    ///
    /// Specifically, this is used by the VRL REPL to incrementally compile
    /// a program as each line is compiled.
    pub(crate) local_env: LocalEnv,
}

impl Program {
    /// Get a reference to the final local environment of the compiler that
    /// compiled the current program.
    pub fn local_env(&self) -> &LocalEnv {
        &self.local_env
    }

    /// Get detailed information about the program, as collected by the VRL
    /// compiler.
    pub fn info(&self) -> &ProgramInfo {
        &self.info
    }

    pub fn resolve(&self, ctx: &mut Context) -> core::Resolved {
        use Runner::*;

        match &self.runner {
            Ast(expressions) => {
                let mut values = expressions
                    .iter()
                    .map(|expr| expr.resolve(ctx))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(values.pop().unwrap_or(Value::Null))
            }
            ValueOrPath(value) => Ok(value
                .as_value(ctx.target())
                .map(|v| v.to_owned())
                .unwrap_or(Value::Null)),
            Comparison(lhs, op, rhs) => {
                const NULL: Value = Value::Null;

                let target = ctx.target();
                let lhs = lhs.as_value(target).unwrap_or(&NULL);
                let rhs = rhs.as_value(target).unwrap_or(&NULL);

                let cmp = match op {
                    Op::Eq => lhs == rhs,
                    Op::Ne => lhs != rhs,
                };

                Ok(cmp.into())
            }
            PathAssignment {
                target,
                value,
                if_exists,
            } => {
                let value = match value.as_value(ctx.target()) {
                    Some(value) => value.to_owned(),
                    None if !*if_exists => Value::Null,
                    None => return Ok(Value::Null),
                };

                ctx.target_mut().target_insert(target, value.clone())?;
                Ok(value)
            }
        }
    }

    pub fn compile_to_vm(&self, vm: &mut vm::Vm, external: &mut ExternalEnv) -> Result<(), String> {
        let mut local = LocalEnv::default();

        let expr: Expr = match &self.runner {
            Runner::Ast(expressions) => {
                for expr in expressions {
                    expr.compile_to_vm(vm, (&mut local, external))?;
                }

                return Ok(());
            }
            Runner::ValueOrPath(value) => value.clone().into(),
            Runner::Comparison(lhs, op, rhs) => {
                let lhs = Box::new(lhs.clone().into());
                let rhs = Box::new(rhs.clone().into());

                let opcode = match op {
                    Op::Eq => Opcode::Eq,
                    Op::Ne => Opcode::Ne,
                };

                expression::Op { lhs, rhs, opcode }.into()
            }
            Runner::PathAssignment {
                target,
                value,
                if_exists: _,
            } => {
                let variant = assignment::Variant::Single {
                    target: {
                        let path = if target.is_root() {
                            None
                        } else {
                            Some(target.clone())
                        };

                        assignment::Target::External(path)
                    },
                    expr: Box::new(Expr::from(value.clone())),
                };

                expression::Assignment { variant }.into()
            }
        };

        expr.compile_to_vm(vm, (&mut local, external))
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Runner {
    /// A "complex" program that needs the full runtime to be resolved.
    Ast(Vec<Box<dyn Expression>>),

    /// A single "path or value" expression.
    ValueOrPath(ValueOrPath),

    /// A single comparison expression between two path-or-value types.
    Comparison(ValueOrPath, Op, ValueOrPath),

    /// A single assignment expression, assigning a path-or-value type to
    /// a target.
    PathAssignment {
        target: LookupBuf,
        value: ValueOrPath,
        if_exists: bool,
    },
}

impl From<Vec<Expr>> for Runner {
    fn from(expressions: Vec<Expr>) -> Self {
        match simple_runner(&expressions) {
            Some(runner) => runner,
            None => {
                let expressions = expressions
                    .into_iter()
                    .map(|expr| Box::new(expr) as _)
                    .collect();

                Self::Ast(expressions)
            }
        }
    }
}

fn simple_runner(expressions: &[Expr]) -> Option<Runner> {
    // We currently only supprt single-expression programs for non-runtime runs.
    if expressions.len() != 1 {
        return None;
    }

    let runner = match &expressions[0] {
        // literal return value
        //
        // ```
        // "foo bar"
        // ```
        expr @ Expr::Literal(_) | expr @ Expr::Query(_) => {
            Runner::ValueOrPath(expr.clone().try_into().ok()?)
        }

        // eq or ne operation
        //
        // ```
        // .foo != "bar"
        // .bar == true
        // ```
        Expr::Op(expression::Op { lhs, rhs, opcode }) => {
            let lhs = (*lhs.to_owned()).try_into().ok()?;
            let rhs = (*rhs.to_owned()).try_into().ok()?;

            match opcode {
                Opcode::Eq => Runner::Comparison(lhs, Op::Eq, rhs),
                Opcode::Ne => Runner::Comparison(lhs, Op::Ne, rhs),
                _ => return None,
            }
        }

        // single-expression Assignment
        //
        // ```
        // .bar = .foo
        // ```
        Expr::Assignment(assignment) => match &assignment.variant {
            assignment::Variant::Single {
                target: assignment::Target::External(target),
                expr,
            } => Runner::PathAssignment {
                target: target.clone().unwrap_or_else(LookupBuf::root),
                value: (*expr.to_owned()).try_into().ok()?,
                if_exists: false,
            },
            _ => return None,
        },

        // if-statement wrapping an assignment, where the predicate is the
        // `exists` function-call, and its argument is the same path as the
        // to-be-assigned value.
        //
        // ```
        // if exists(.foo) {
        //   .bar = .foo
        // }
        // ```
        Expr::IfStatement(_stmt) => return None,
        _ => return None,
    };

    Some(runner)
}

/// A simplified equality check.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Op {
    Eq,
    Ne,
}

/// A type representing either a known [`Value`] type, or a [`LookupBuf`] into
/// a potential `Value` into the external target.
#[derive(Debug, Clone)]
pub(crate) enum ValueOrPath {
    Value(Value),
    Path(LookupBuf),
}

impl ValueOrPath {
    fn as_value<'a>(&'a self, target: &'a dyn Target) -> Option<&'a Value> {
        match self {
            Self::Value(value) => Some(value),
            Self::Path(path) => target.target_get(path).ok().flatten(),
        }
    }
}

impl From<Value> for ValueOrPath {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl From<LookupBuf> for ValueOrPath {
    fn from(path: LookupBuf) -> Self {
        Self::Path(path)
    }
}

impl TryFrom<Expr> for ValueOrPath {
    type Error = ();

    fn try_from(expr: Expr) -> Result<Self, Self::Error> {
        match expr {
            Expr::Literal(lit) => lit.as_value().map(Into::into).ok_or(()),
            Expr::Query(query) if query.is_external() => Ok(query.path().clone().into()),
            _ => Err(()),
        }
    }
}

impl From<ValueOrPath> for Expr {
    fn from(value: ValueOrPath) -> Self {
        match value {
            ValueOrPath::Value(value) => value.into(),
            ValueOrPath::Path(path) => Self::Query(expression::Query::new(
                expression::query::Target::External,
                path.clone(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgramInfo {
    /// Returns whether the compiled program can fail at runtime.
    ///
    /// A program can only fail at runtime if the fallible-function-call
    /// (`foo!()`) is used within the source.
    pub fallible: bool,

    /// Returns whether the compiled program can be aborted at runtime.
    ///
    /// A program can only abort at runtime if there's an explicit `abort`
    /// statement in the source.
    pub abortable: bool,

    /// A list of possible queries made to the external [`Target`] at runtime.
    pub target_queries: Vec<LookupBuf>,

    /// A list of possible assignments made to the external [`Target`] at
    /// runtime.
    pub target_assignments: Vec<LookupBuf>,
}
