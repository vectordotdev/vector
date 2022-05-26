use bytes::{BufMut, BytesMut};
use std::{any::Any, collections::BTreeMap};
use vrl_core::{
    Context, Error, ExpressionError, LookupBuf, Resolved, Span, Target, Value, VrlValueArithmetic,
};

// We only want to precompile the stub for this function, and therefore don't
// reference the function arguments.
// The function body will be implemented in our code generation framework.
#[allow(unused_variables)]
#[no_mangle]
pub extern "C" fn vrl_execute(context: &mut Context, result: &mut Resolved) {}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_resolved_initialize(result: &mut Resolved) {
    let result = result as *mut Resolved;
    result.write(Ok(Value::Null));
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_value_initialize(result: &mut Value) {
    let result = result as *mut Value;
    result.write(Value::Null);
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_optional_value_initialize(result: &mut Option<Value>) {
    let result = result as *mut Option<Value>;
    result.write(None);
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_static_initialize(result: &mut Box<dyn Any + Send + Sync>) {
    let result = result as *mut Box<dyn Any + Send + Sync>;
    result.write(Box::new(()));
}

#[no_mangle]
pub extern "C" fn vrl_resolved_as_value(result: &mut Resolved) -> &mut Value {
    match result {
        Ok(value) => value,
        Err(_) => panic!(r#"expected value "{:?}" not to be an error"#, result),
    }
}

#[no_mangle]
pub extern "C" fn vrl_resolved_as_value_to_optional_value(
    result: &mut Resolved,
    optional_value: &mut Option<Value>,
) {
    let result = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(result, &mut moved);
        moved
    };
    *optional_value = match result {
        Ok(value) => Some(value),
        Err(_) => panic!(r#"expected value "{:?}" not to be an error"#, result),
    }
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_resolved_drop(result: &mut Resolved) {
    let result = result as *mut Resolved;
    drop(result.read());
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_optional_value_drop(result: &mut Option<Value>) {
    let result = result as *mut Option<Value>;
    drop(result.read());
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
pub extern "C" fn vrl_value_boolean_is_true(result: &Resolved) -> bool {
    result
        .as_ref()
        .expect("VRL result must not contain an error")
        .as_boolean()
        .expect("VRL value must be boolean")
}

#[no_mangle]
pub extern "C" fn vrl_value_is_falsy(result: &Resolved) -> bool {
    result
        .as_ref()
        .expect("VRL result must not contain an error")
        .is_falsy()
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_btree_map_initialize(map: &mut BTreeMap<String, Value>) {
    let map = map as *mut BTreeMap<String, Value>;
    map.write(Default::default());
}

#[no_mangle]
pub extern "C" fn vrl_target_assign(value: &Resolved, target: &mut Resolved) {
    *target = value.clone()
}

#[no_mangle]
pub extern "C" fn vrl_btree_map_insert(
    map: &mut BTreeMap<String, Value>,
    #[allow(clippy::ptr_arg)] key: &String,
    result: &mut Resolved,
) {
    let result = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(result, &mut moved);
        moved
    };
    map.insert(
        key.clone(),
        result.expect("VRL result must not contain an error"),
    );
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

#[no_mangle]
pub extern "C" fn vrl_expression_not_impl(result: &mut Resolved) {
    *result = Ok((!result
        .as_ref()
        .expect("VRL result must not contain an error")
        .as_boolean()
        .expect("VRL value must be boolean"))
    .into());
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_object_set_result_impl(
    map: &mut BTreeMap<String, Value>,
    result: &mut Resolved,
) {
    let map = map as *mut BTreeMap<String, Value>;
    *result = Ok(Value::Object(map.read()));
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_mul_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs * rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_mul_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs * rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_mul_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_mul(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_div_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = if rhs == 0 {
        Err(Error::DivideByZero.into())
    } else {
        Ok((lhs / rhs).into())
    }
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_div_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = if rhs == 0.0 {
        Err(Error::DivideByZero.into())
    } else {
        Ok((lhs / rhs).into())
    }
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_div_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_div(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_add_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs + rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_add_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs + rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_add_bytes_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    let mut value = BytesMut::with_capacity(lhs.len() + rhs.len());
    value.put(lhs);
    value.put(rhs);

    *result = Ok(value.freeze().into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_add_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_add(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_sub_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs - rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_sub_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs - rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_sub_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_sub(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_rem_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs % rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_rem_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs % rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_rem_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_rem(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_ne_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs != rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_ne_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs != rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_ne_bytes_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    *result = Ok((lhs != rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_ne_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = Ok((!lhs.eq_lossy(&rhs)).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_eq_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs == rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_eq_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs == rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_eq_bytes_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    *result = Ok((lhs == rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_eq_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = Ok((lhs.eq_lossy(&rhs)).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_ge_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs >= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_ge_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs >= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_ge_bytes_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    *result = Ok((lhs >= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_ge_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_ge(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_gt_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs > rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_gt_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs > rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_gt_bytes_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    *result = Ok((lhs > rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_gt_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_gt(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_le_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs <= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_le_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs <= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_le_bytes_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    *result = Ok((lhs <= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_le_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_le(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_lt_integer_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    *result = Ok((lhs <= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_lt_float_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Float(float) => float,
        _ => panic!(),
    };

    *result = Ok((lhs <= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_lt_bytes_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    *result = Ok((lhs <= rhs).into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_lt_impl(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_lt(rhs).map_err(Into::into)
}

#[no_mangle]
pub extern "C" fn vrl_expression_query_target_external_impl(
    context: &mut Context,
    path: &LookupBuf,
    result: &mut Resolved,
) {
    *result = Ok(context
        .target
        .target_get(path)
        .ok()
        .flatten()
        .map(Clone::clone)
        .unwrap_or(Value::Null));
}

#[no_mangle]
pub extern "C" fn vrl_expression_query_target_impl(path: &LookupBuf, result: &mut Resolved) {
    *result = Ok(result
        .as_ref()
        .expect("VRL result must not contain an error")
        .target_get(path)
        .ok()
        .flatten()
        .map(Clone::clone)
        .unwrap_or(Value::Null));
}
