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

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_vec_initialize(vec: &mut Vec<Value>, capacity: usize) {
    let vec = vec as *mut Vec<Value>;
    vec.write(
        #[allow(clippy::uninit_vec)]
        {
            let mut vec = Vec::with_capacity(capacity);
            vec.set_len(capacity);
            vec
        },
    );
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_btree_map_initialize(vec: &mut Vec<(String, Value)>, capacity: usize) {
    let vec = vec as *mut Vec<(String, Value)>;
    vec.write(
        #[allow(clippy::uninit_vec)]
        {
            let mut vec = Vec::with_capacity(capacity);
            vec.set_len(capacity);
            vec
        },
    );
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

#[no_mangle]
pub extern "C" fn vrl_target_assign(value: &Resolved, target: &mut Resolved) {
    *target = value.clone()
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_vec_insert(vec: &mut Vec<Value>, index: usize, value: &mut Resolved) {
    let value = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(value, &mut moved);
        moved
    };
    vec.as_mut_ptr()
        .add(index)
        .write(value.expect("VRL result must not contain an error"));
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_btree_map_insert(
    vec: &mut Vec<(String, Value)>,
    index: usize,
    #[allow(clippy::ptr_arg)] key: &String,
    value: &mut Resolved,
) {
    let value = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(value, &mut moved);
        moved
    };
    vec.as_mut_ptr().add(index).write((
        key.clone(),
        value.expect("VRL result must not contain an error"),
    ));
}

#[no_mangle]
pub extern "C" fn vrl_resolved_set_null(result: &mut Resolved) {
    *result = Ok(Value::Null)
}

#[no_mangle]
pub extern "C" fn vrl_resolved_set_false(result: &mut Resolved) {
    *result = Ok(false.into())
}

#[no_mangle]
pub extern "C" fn vrl_expression_abort(span: &Span, message: &Resolved, result: &mut Resolved) {
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
pub extern "C" fn vrl_expression_assignment_target_insert_internal_path(
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
pub extern "C" fn vrl_expression_assignment_target_insert_external(
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
pub extern "C" fn vrl_expression_literal(value: &Value, result: &mut Resolved) {
    *result = Ok(value.clone());
}

#[no_mangle]
pub extern "C" fn vrl_expression_not(result: &mut Resolved) {
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
pub unsafe extern "C" fn vrl_expression_array_set_result(
    vec: &mut Vec<Value>,
    result: &mut Resolved,
) {
    let vec = vec as *mut Vec<Value>;
    *result = Ok(Value::Array(vec.read()));
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_object_set_result(
    vec: &mut Vec<(String, Value)>,
    result: &mut Resolved,
) {
    let vec = (vec as *mut Vec<(String, Value)>).read();
    *result = Ok(Value::Object(vec.into_iter().collect()));
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_mul_integer(
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
pub unsafe extern "C" fn vrl_expression_op_mul_float(
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
pub unsafe extern "C" fn vrl_expression_op_mul(
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
pub unsafe extern "C" fn vrl_expression_op_div_integer(
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
pub unsafe extern "C" fn vrl_expression_op_div_float(
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
pub unsafe extern "C" fn vrl_expression_op_div(
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
pub unsafe extern "C" fn vrl_expression_op_add_integer(
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
pub unsafe extern "C" fn vrl_expression_op_add_float(
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
pub unsafe extern "C" fn vrl_expression_op_add_bytes(
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
pub unsafe extern "C" fn vrl_expression_op_add(
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
pub unsafe extern "C" fn vrl_expression_op_sub_integer(
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
pub unsafe extern "C" fn vrl_expression_op_sub_float(
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
pub unsafe extern "C" fn vrl_expression_op_sub(
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
pub unsafe extern "C" fn vrl_expression_op_rem_integer(
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
pub unsafe extern "C" fn vrl_expression_op_rem_float(
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
pub unsafe extern "C" fn vrl_expression_op_rem(
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
pub unsafe extern "C" fn vrl_expression_op_ne_integer(
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
pub unsafe extern "C" fn vrl_expression_op_ne_float(
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
pub unsafe extern "C" fn vrl_expression_op_ne_bytes(
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
pub unsafe extern "C" fn vrl_expression_op_ne(
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
pub unsafe extern "C" fn vrl_expression_op_eq_integer(
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
pub unsafe extern "C" fn vrl_expression_op_eq_float(
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
pub unsafe extern "C" fn vrl_expression_op_eq_bytes(
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
pub unsafe extern "C" fn vrl_expression_op_eq(
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
pub unsafe extern "C" fn vrl_expression_op_ge_integer(
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
pub unsafe extern "C" fn vrl_expression_op_ge_float(
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
pub unsafe extern "C" fn vrl_expression_op_ge_bytes(
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
pub unsafe extern "C" fn vrl_expression_op_ge(
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
pub unsafe extern "C" fn vrl_expression_op_gt_integer(
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
pub unsafe extern "C" fn vrl_expression_op_gt_float(
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
pub unsafe extern "C" fn vrl_expression_op_gt_bytes(
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
pub unsafe extern "C" fn vrl_expression_op_gt(
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
pub unsafe extern "C" fn vrl_expression_op_le_integer(
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
pub unsafe extern "C" fn vrl_expression_op_le_float(
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
pub unsafe extern "C" fn vrl_expression_op_le_bytes(
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
pub unsafe extern "C" fn vrl_expression_op_le(
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
pub unsafe extern "C" fn vrl_expression_op_lt_integer(
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
pub unsafe extern "C" fn vrl_expression_op_lt_float(
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
pub unsafe extern "C" fn vrl_expression_op_lt_bytes(
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
pub unsafe extern "C" fn vrl_expression_op_lt(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Value).read();
    let rhs = (rhs as *mut Value).read();

    *result = lhs.try_lt(rhs).map_err(Into::into)
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_merge_object(
    lhs: &mut Value,
    rhs: &mut Value,
    result: &mut Resolved,
) {
    let lhs = match (lhs as *mut Value).read() {
        Value::Object(object) => object,
        _ => panic!(),
    };
    let rhs = match (rhs as *mut Value).read() {
        Value::Object(object) => object,
        _ => panic!(),
    };

    *result = Ok(lhs
        .iter()
        .chain(rhs.iter())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<String, Value>>()
        .into())
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_and_truthy(
    lhs: &mut Resolved,
    rhs: &mut Resolved,
    result: &mut Resolved,
) {
    let lhs = (lhs as *mut Resolved).read();
    let rhs = (rhs as *mut Resolved).read();

    *result = (|| lhs?.try_and(rhs?).map_err(Into::into))()
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_expression_op_and_falsy(lhs: &mut Resolved, result: &mut Resolved) {
    let lhs = (lhs as *mut Resolved).read();
    drop(lhs);

    *result = Ok(false.into())
}

#[no_mangle]
pub extern "C" fn vrl_expression_query_target_external(
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
pub extern "C" fn vrl_expression_query_target(path: &LookupBuf, result: &mut Resolved) {
    *result = Ok(result
        .as_ref()
        .expect("VRL result must not contain an error")
        .target_get(path)
        .ok()
        .flatten()
        .map(Clone::clone)
        .unwrap_or(Value::Null));
}

#[no_mangle]
pub extern "C" fn vrl_expression_function_call(
    #[allow(clippy::ptr_arg)] error: &String,
    result: &mut Resolved,
) {
    if let Err(ExpressionError::Error {
        message,
        labels,
        notes,
    }) = result
    {
        *result = Err(ExpressionError::Error {
            message: format!(r#"{}: {}"#, error, message),
            labels: std::mem::take(labels),
            notes: std::mem::take(notes),
        })
    }
}

#[no_mangle]
pub extern "C" fn vrl_del_external(context: &mut Context, path: &LookupBuf, result: &mut Resolved) {
    *result = Ok(context
        .target
        .target_remove(path, false)
        .ok()
        .flatten()
        .unwrap_or(Value::Null))
}

#[no_mangle]
pub extern "C" fn vrl_del_internal(
    variable: &mut Resolved,
    path: &LookupBuf,
    result: &mut Resolved,
) {
    let value = variable.as_mut().unwrap();
    let new_value = value.get_by_path(path).cloned();
    value.remove_by_path(path, false);
    *result = Ok(new_value.unwrap_or(Value::Null));
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_del_expression(
    expression: &mut Resolved,
    path: &LookupBuf,
    result: &mut Resolved,
) {
    let value = (expression as *mut Resolved)
        .read()
        .expect("expression must not contain an error");
    // No need to do the actual deletion, as the expression is only
    // available as an argument to the function.
    *result = Ok(value.get_by_path(path).cloned().unwrap_or(Value::Null));
}

#[no_mangle]
pub extern "C" fn vrl_exists_external(
    context: &mut Context,
    path: &LookupBuf,
    result: &mut Resolved,
) {
    *result = Ok(context
        .target
        .target_get(path)
        .ok()
        .flatten()
        .is_some()
        .into())
}

#[no_mangle]
pub extern "C" fn vrl_exists_internal(
    variable: &mut Resolved,
    path: &LookupBuf,
    result: &mut Resolved,
) {
    let value = variable.as_mut().unwrap();
    *result = Ok(value.get_by_path(path).is_some().into());
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_exists_expression(
    expression: &mut Resolved,
    path: &LookupBuf,
    result: &mut Resolved,
) {
    let value = (expression as *mut Resolved)
        .read()
        .expect("expression must not contain an error");
    // No need to do the actual deletion, as the expression is only
    // available as an argument to the function.
    *result = Ok(value.get_by_path(path).is_some().into());
}
