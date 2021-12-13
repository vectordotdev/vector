use vrl_core::{Context, ExpressionError, LookupBuf, Resolved, Span, Value};

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
    *result = Ok(match result {
        Err(error) => error.to_string().into(),
        _ => panic!(r#"expected value "{:?}" to be an error"#, result),
    })
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
    result
        .as_ref()
        .expect("VRL result must not contain an error")
        .as_boolean()
        .expect("VRL value must be boolean")
}

#[no_mangle]
pub extern "C" fn vrl_expression_abort_impl(
    span: &Span,
    message: &Resolved,
    result: &mut Resolved,
) {
    let message = match message {
        Ok(Value::Null) => None,
        Ok(Value::Bytes(bytes)) => Some(String::from_utf8_lossy(bytes).into()),
        _ => panic!(r#"abort message "{:?}" is not a string"#, message),
    };
    *result = Err(ExpressionError::Abort {
        span: *span,
        message,
    });
}

#[no_mangle]
pub extern "C" fn vrl_expression_assignment_target_insert_internal_impl(
    value: &Resolved,
    target: &mut Resolved,
) {
    *target = value.clone()
}

#[no_mangle]
pub extern "C" fn vrl_expression_assignment_target_insert_internal_path_impl(
    value: &Resolved,
    path: &LookupBuf,
    target: &mut Resolved,
) {
    let value = value
        .as_ref()
        .expect("assignment value must not contain an error");
    let target = target
        .as_mut()
        .expect("assignment target must not contain an error");
    target.insert_by_path(path, value.clone())
}

#[no_mangle]
pub extern "C" fn vrl_expression_assignment_target_insert_external_impl(
    value: &Resolved,
    path: &LookupBuf,
    ctx: &mut Context,
) {
    let value = value
        .as_ref()
        .expect("assignment value must not contain an error");
    let _ = ctx.target.target_insert(path, value.clone());
}

#[no_mangle]
pub extern "C" fn vrl_expression_literal_impl(value: &Value, result: &mut Resolved) {
    *result = Ok(value.clone());
}
