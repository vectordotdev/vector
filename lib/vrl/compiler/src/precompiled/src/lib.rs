use lookup::LookupBuf;
use vrl_compiler::{parser::Ident, Context, ExpressionError, Resolved, Span, Value};

// We only want to precompile the stub for this function, and therefore don't
// reference the function arguments.
// The function body will be implemented in our code generation framework.
#[allow(unused_variables)]
#[no_mangle]
pub extern "C" fn vrl_execute(context: &mut Context, result: &mut Resolved) {}

#[no_mangle]
pub extern "C" fn vrl_resolved_initialize(result: *mut Resolved) {
    unsafe { result.write(Ok(Value::Null)) };
}

#[no_mangle]
pub extern "C" fn vrl_resolved_drop(result: *mut Resolved) {
    drop(unsafe { result.read() });
}

#[no_mangle]
pub extern "C" fn vrl_resolved_err_into_ok(result: &mut Resolved) {
    *result = Ok(result.as_ref().unwrap_err().to_string().into())
}

#[no_mangle]
pub extern "C" fn vrl_resolved_is_ok(result: &mut Resolved) -> bool {
    result.is_ok()
}

#[no_mangle]
pub extern "C" fn vrl_resolved_is_err(result: &mut Resolved) -> bool {
    result.is_err()
}

#[no_mangle]
pub extern "C" fn vrl_resolved_is_boolean(result: &Resolved) -> bool {
    result
        .as_ref()
        .map(|value| value.is_boolean())
        .unwrap_or(false)
}

#[no_mangle]
pub extern "C" fn vrl_resolved_boolean_is_true(result: &Resolved) -> bool {
    result.as_ref().unwrap().as_boolean().unwrap()
}

#[no_mangle]
pub extern "C" fn vrl_expression_abort_impl(span: &Span, result: &mut Resolved) {
    *result = Err(ExpressionError::Abort { span: *span });
}

#[no_mangle]
pub extern "C" fn vrl_expression_assignment_target_insert_internal_impl(
    ctx: &mut Context,
    ident: &Ident,
    resolved: &Resolved,
) {
    let value = resolved.as_ref().unwrap().clone();
    ctx.state_mut().insert_variable(ident.clone(), value);
}

#[no_mangle]
pub extern "C" fn vrl_expression_assignment_target_insert_internal_path_impl(
    ctx: &mut Context,
    ident: &Ident,
    path: &LookupBuf,
    resolved: &Resolved,
) {
    let value = resolved.as_ref().unwrap().clone();
    match ctx.state_mut().variable_mut(ident) {
        Some(stored) => stored.insert_by_path(path, value),
        None => ctx
            .state_mut()
            .insert_variable(ident.clone(), value.at_path(path)),
    }
}

#[no_mangle]
pub extern "C" fn vrl_expression_assignment_target_insert_external_impl(
    ctx: &mut Context,
    path: &LookupBuf,
    resolved: &Resolved,
) {
    let value = resolved.as_ref().unwrap().clone();
    let _ = ctx.target_mut().insert(path, value);
}
