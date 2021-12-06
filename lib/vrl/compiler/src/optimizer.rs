use crate::expression::{Assignment, Expr};

pub(crate) fn optimize(expressions: Vec<Expr>) -> Vec<Expr> {
    use Expr::*;

    let mut out = Vec::with_capacity(expressions.len());
    let mut expressions = expressions.into_iter().peekable();

    while let Some(expr) = expressions.next() {
        let expr = match expr {
            Assignment(v) if expressions.peek().is_some() => optimize_root_assignment(v).into(),
            _ => expr,
        };

        out.push(expr);
    }

    out
}

/// If an assignment happens at the "root" level, and the assignment isn't the last expression in
/// the program, then the return value of the assignment can be omitted, as it won't alter the
/// outcome of the program.
///
/// By doing so, we avoid an extra clone of the value.
///
/// TODO: ideally we could generalize this to apply to _all_ expressions.
/// TODO: do the same for blocks, when the assignment isn't the last "root"
///       expression in the block.
fn optimize_root_assignment(mut assignment: Assignment) -> Assignment {
    assignment.omit_return_value(true);
    assignment
}
