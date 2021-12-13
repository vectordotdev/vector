use vrl_compiler::{Context, Resolved};

// We only want to precompile the stub for this function, and therefore don't
// reference the function arguments.
// The function body will be implemented in our code generation framework.
#[allow(unused_variables)]
#[no_mangle]
pub extern "C" fn vrl_execute(context: &mut Context, result: &mut Resolved) {}

#[no_mangle]
pub extern "C" fn vrl_expression_abort_impl(span: &Span, result: &mut Resolved) {
    *result = Err(ExpressionError::Abort { span: *span });
}
