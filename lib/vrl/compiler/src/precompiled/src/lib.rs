use std::{collections::BTreeMap, ffi::c_void};
use vrl_compiler::{
    expression::assignment::Target as AssignmentTarget, parser::Ident, Context, ExpressionError,
    Resolved, Span, Target, Value,
};

// https://github.com/rust-lang/rust/issues/59164
#[cfg(target_os = "macos")]
#[no_mangle]
extern "C" fn __emutls_get_address(_: *const c_void) -> *const c_void {
    unimplemented!()
}

#[allow(unused_variables)]
#[no_mangle]
pub extern "C" fn vrl_execute(context: &mut Context, result: &mut Resolved) {}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_resolved_initialize(result: *mut Resolved) {
    unsafe { result.write(Ok(Value::Null)) };
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_resolved_drop(result: *mut Resolved) {
    drop(unsafe { result.read() });
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_resolved_is_err(result: &Resolved) -> bool {
    result.is_err()
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_resolved_is_boolean(result: &Resolved) -> bool {
    result
        .as_ref()
        .map(|value| value.is_boolean())
        .unwrap_or(false)
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_resolved_boolean_is_true(result: &Resolved) -> bool {
    result.as_ref().unwrap().as_boolean().unwrap()
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_btree_map_initialize(map: *mut BTreeMap<String, Value>) {
    unsafe { map.write(Default::default()) };
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_btree_map_drop(map: *mut BTreeMap<String, Value>) {
    drop(unsafe { map.read() });
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_btree_map_insert(
    map: &mut BTreeMap<String, Value>,
    key: &String,
    resolved: &mut Resolved,
) {
    let resolved = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(resolved, &mut moved);
        moved
    };
    map.insert(key.clone(), resolved.unwrap());
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_abort_impl(span: &Span, result: &mut Resolved) {
    *result = Err(ExpressionError::Abort { span: *span });
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_assignment_variant_single_impl(
    target: &AssignmentTarget,
    ctx: &mut Context,
    result: &mut Resolved,
) {
    if let Ok(value) = result {
        target.insert(value.clone(), ctx);
    }
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_assignment_variant_infallible_impl(
    ok: &AssignmentTarget,
    err: &AssignmentTarget,
    default: &Value,
    ctx: &mut Context,
    result: &mut Resolved,
) {
    match result {
        Ok(value) => {
            ok.insert(value.clone(), ctx);
            err.insert(Value::Null, ctx);
        }
        Err(error) => {
            ok.insert(default.clone(), ctx);
            let value = Value::from(error.to_string());
            err.insert(value.clone(), ctx);
            *result = Ok(value);
        }
    }
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_literal_impl(value: &Value, result: &mut Resolved) {
    *result = Ok(value.clone());
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_not_impl(result: &mut Resolved) {
    if let Ok(value) = result {
        *result = value
            .clone()
            .try_boolean()
            .map(|boolean| (!boolean).into())
            .map_err(Into::into);
    }
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_object_set_result_impl(
    map: &mut BTreeMap<String, Value>,
    result: &mut Resolved,
) {
    let map = {
        let mut moved = Default::default();
        std::mem::swap(map, &mut moved);
        moved
    };
    *result = Ok(Value::Object(map));
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_op_add_impl(rhs: &mut Resolved, result: &mut Resolved) {
    let rhs = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(rhs, &mut moved);
        moved
    };
    match (&result, rhs) {
        (Ok(lhs), Ok(rhs)) => {
            *result = lhs.clone().try_add(rhs).map_err(Into::into);
        }
        (Err(_), Err(_)) => (),
        (Err(_), _) => (),
        (_, Err(error)) => {
            *result = Err(error);
        }
    }
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_op_eq_impl(rhs: &mut Resolved, result: &mut Resolved) {
    let rhs = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(rhs, &mut moved);
        moved
    };
    match (&result, rhs) {
        (Ok(lhs), Ok(rhs)) => {
            *result = Ok(lhs.eq_lossy(&rhs).into());
        }
        (Err(_), Err(_)) => (),
        (Err(_), _) => (),
        (_, Err(error)) => {
            *result = Err(error);
        }
    }
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_query_target_external_impl(
    context: &mut Context,
    path: &lookup::LookupBuf,
    result: &mut Resolved,
) {
    *result = Ok(context
        .target()
        .get(path)
        .ok()
        .flatten()
        .unwrap_or(Value::Null));
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_query_target_impl(
    path: &lookup::LookupBuf,
    result: &mut Resolved,
) {
    *result = Ok(result
        .as_ref()
        .unwrap()
        .get(path)
        .ok()
        .flatten()
        .unwrap_or(Value::Null));
}

#[inline]
#[no_mangle]
pub extern "C" fn vrl_expression_variable_impl(
    context: &mut Context,
    ident: &Ident,
    result: &mut Resolved,
) {
    *result = Ok(context
        .state()
        .variable(ident)
        .cloned()
        .unwrap_or(Value::Null));
}
