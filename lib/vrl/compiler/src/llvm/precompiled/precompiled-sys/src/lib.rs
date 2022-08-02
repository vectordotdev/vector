use bytes::{BufMut, BytesMut};
use primitive_calling_convention::primitive_calling_convention;
use std::{any::Any, collections::BTreeMap, mem::MaybeUninit};
use vrl_core::{
    Context, Error, ExpressionError, LookupBuf, Resolved, Span, Value, VrlValueArithmetic,
    VrlValueConvert,
};

#[no_mangle]
#[allow(clippy::ptr_arg)]
#[primitive_calling_convention]
pub extern "C" fn vrl_types(
    _resolved_ref: &Resolved,
    _resolved_mut: &mut Resolved,
    _resolved_maybe_uninit_mut: &mut MaybeUninit<Resolved>,
    _value_ref: &Value,
    _value_mut: &mut Value,
    _optional_value_mut: &mut Option<Value>,
    _optional_value_maybe_uninit_mut: &mut MaybeUninit<Option<Value>>,
    _static_argument_ref: &Box<dyn Any + Send + Sync>,
    _vec_mut: &mut Vec<Resolved>,
    _vec_maybe_uninit_mut: &mut MaybeUninit<Vec<MaybeUninit<Resolved>>>,
    _btree_map_mut: &mut Vec<(String, Value)>,
    _btree_map_maybe_uninit_mut: &mut MaybeUninit<Vec<MaybeUninit<(String, Value)>>>,
    _span_ref: &Span,
    _lookup_buf_ref: &LookupBuf,
    _string_ref: &String,
) {
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_execute(_: &mut Context) -> Resolved {
    Ok(Value::Null)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_initialize(resolved: &mut MaybeUninit<Resolved>) {
    resolved.write(Ok(Value::Null));
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_optional_value_initialize(optional_value: &mut MaybeUninit<Option<Value>>) {
    optional_value.write(None);
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_vec_initialize(vec: &mut MaybeUninit<Vec<Value>>, len: usize) {
    vec.write(Vec::with_capacity(len));
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_btree_map_initialize(
    btree_map: &mut MaybeUninit<Vec<MaybeUninit<(String, Value)>>>,
    len: usize,
) {
    btree_map.write(Vec::with_capacity(len));
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_drop(resolved: Resolved) {
    drop(resolved);
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_optional_value_drop(optional_value: Option<Value>) {
    drop(optional_value);
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_vec_drop(vec: Vec<Value>) {
    drop(vec);
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_btree_map_drop(btree_map: Vec<(String, Value)>) {
    drop(btree_map);
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_move(resolved: Resolved) -> Resolved {
    resolved
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_as_value(resolved: &mut Resolved) -> &mut Value {
    match resolved {
        Ok(value) => value,
        Err(_) => panic!(r#"expected value "{:?}" not to be an error"#, resolved),
    }
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_as_value_to_optional_value(resolved: Resolved) -> Option<Value> {
    match resolved {
        Ok(value) => Some(value),
        Err(_) => panic!(r#"expected value "{:?}" not to be an error"#, resolved),
    }
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_err_into_ok(resolved: &mut Resolved) {
    *resolved = Ok(match resolved {
        Err(error) => error.to_string().into(),
        _ => panic!(r#"expected value "{:?}" to be an error"#, resolved),
    })
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_is_ok(resolved: &Resolved) -> bool {
    resolved.is_ok()
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_is_err(resolved: &Resolved) -> bool {
    resolved.is_err()
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_is_abort(resolved: &Resolved) -> bool {
    matches!(resolved, Err(error) if error.is_abort())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_value_boolean_is_true(resolved: &Resolved) -> bool {
    resolved
        .as_ref()
        .expect("VRL resolved value must not contain an error")
        .as_boolean()
        .expect("VRL value must be boolean")
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_value_is_falsy(resolved: &Resolved) -> bool {
    resolved
        .as_ref()
        .expect("VRL resolved value must not contain an error")
        .is_falsy()
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_swap(x: &mut Resolved, y: &mut Resolved) {
    std::mem::swap(x, y)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_clone(resolved: &Resolved) -> Resolved {
    resolved.clone()
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_vec_push(#[allow(clippy::ptr_arg)] vec: &mut Vec<Value>, resolved: Resolved) {
    vec.push(resolved.expect("VRL resolved value must not contain an error"));
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_btree_map_push(
    #[allow(clippy::ptr_arg)] btree_map: &mut Vec<(String, Value)>,
    #[allow(clippy::ptr_arg)] key: &String,
    resolved: Resolved,
) {
    btree_map.push((
        key.clone(),
        resolved.expect("VRL resolved value must not contain an error"),
    ));
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_ok_null() -> Resolved {
    Ok(Value::Null)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_resolved_ok_false() -> Resolved {
    Ok(false.into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_abort(span: &Span, message: Resolved) -> Resolved {
    let message =
        (|| Ok::<_, ExpressionError>(message?.try_bytes_utf8_lossy()?.to_string()))().ok();
    Err(ExpressionError::abort(*span, message.as_deref()))
}

#[no_mangle]
#[primitive_calling_convention]
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
#[primitive_calling_convention]
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
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_literal(value: &Value) -> Resolved {
    Ok(value.clone())
}

#[no_mangle]
pub extern "C" fn vrl_expression_not(result: &mut Resolved) {
    *result = Ok((!result
        .as_ref()
        .expect("VRL resolved value must not contain an error")
        .as_boolean()
        .expect("VRL value must be boolean"))
    .into());
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_array_into_result(entries: Vec<Value>) -> Resolved {
    Ok(Value::Array(entries))
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_object_into_result(entries: Vec<(String, Value)>) -> Resolved {
    Ok(Value::Object(entries.into_iter().collect()))
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_mul_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs * rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_mul_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs * rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_mul(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_mul(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_div_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    if rhs == 0 {
        Err(Error::DivideByZero.into())
    } else {
        Ok(Value::from_f64_or_zero(lhs as f64 / rhs as f64))
    }
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_div_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    if rhs == 0.0 {
        Err(Error::DivideByZero.into())
    } else {
        Ok((lhs / rhs).into())
    }
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_div(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_div(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_add_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs + rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_add_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs + rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_add_bytes(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    let mut value = BytesMut::with_capacity(lhs.len() + rhs.len());
    value.put(lhs);
    value.put(rhs);

    Ok(value.freeze().into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_add(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_add(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_sub_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs - rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_sub_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs - rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_sub(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_sub(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_rem_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    if rhs == 0 {
        Err(Error::DivideByZero.into())
    } else {
        Ok((lhs % rhs).into())
    }
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_rem_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    if rhs == 0.0 {
        Err(Error::DivideByZero.into())
    } else {
        Ok((lhs % rhs).into())
    }
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_rem(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_rem(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_ne_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs != rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_ne_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs != rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_ne_bytes(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    Ok((lhs != rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_ne(lhs: Value, rhs: Value) -> Resolved {
    Ok((!lhs.eq_lossy(&rhs)).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_eq_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs == rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_eq_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs == rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_eq_bytes(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    Ok((lhs == rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_eq(lhs: Value, rhs: Value) -> Resolved {
    Ok((lhs.eq_lossy(&rhs)).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_ge_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs >= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_ge_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs >= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_ge_bytes(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    Ok((lhs >= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_ge(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_ge(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_gt_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs > rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_gt_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs > rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_gt_bytes(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    Ok((lhs > rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_gt(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_gt(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_le_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs <= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_le_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs <= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_le_bytes(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    Ok((lhs <= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_le(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_le(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_lt_integer(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Integer(integer) => integer,
        _ => panic!(),
    };

    Ok((lhs <= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_lt_float(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Float(float) => float,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Float(float) => float,
        _ => panic!(),
    };

    Ok((lhs <= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_lt_bytes(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Bytes(bytes) => bytes,
        _ => panic!(),
    };

    Ok((lhs <= rhs).into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_lt(lhs: Value, rhs: Value) -> Resolved {
    lhs.try_lt(rhs).map_err(Into::into)
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_merge_object(lhs: Value, rhs: Value) -> Resolved {
    let lhs = match lhs {
        Value::Object(object) => object,
        _ => panic!(),
    };
    let rhs = match rhs {
        Value::Object(object) => object,
        _ => panic!(),
    };

    Ok(lhs
        .iter()
        .chain(rhs.iter())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<String, Value>>()
        .into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_and_truthy(lhs: Resolved, rhs: Resolved) -> Resolved {
    (|| lhs?.try_and(rhs?).map_err(Into::into))()
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_op_and_falsy(lhs: Resolved) -> Resolved {
    drop(lhs);

    Ok(false.into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_query_target_external(
    context: &mut Context,
    path: &LookupBuf,
) -> Resolved {
    Ok(context
        .target
        .target_get(path)
        .ok()
        .flatten()
        .map(Clone::clone)
        .unwrap_or(Value::Null))
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_query_target(path: &LookupBuf, result: &mut Resolved) {
    *result = Ok(result
        .as_ref()
        .expect("VRL resolved value must not contain an error")
        .get_by_path(path)
        .cloned()
        .unwrap_or(Value::Null));
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_expression_function_call_abort(
    #[allow(clippy::ptr_arg)] ident: &String,
    span: &Span,
    abort_on_error: bool,
    resolved: &mut Resolved,
) {
    let error = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(resolved, &mut moved);
        moved
    };

    *resolved = Err(ExpressionError::function_abort(
        *span,
        ident,
        abort_on_error,
        error.expect_err("VRL resolved value must contain an error"),
    ));
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_del_external(context: &mut Context, path: &LookupBuf) -> Resolved {
    Ok(context
        .target
        .target_remove(path, false)
        .ok()
        .flatten()
        .unwrap_or(Value::Null))
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_del_internal(variable: &mut Resolved, path: &LookupBuf) -> Resolved {
    let value = variable.as_mut().unwrap();
    let new_value = value.get_by_path(path).cloned();
    value.remove_by_path(path, false);
    Ok(new_value.unwrap_or(Value::Null))
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_del_expression(expression: Resolved, path: &LookupBuf) -> Resolved {
    let value = expression.expect("expression must not contain an error");
    // No need to do the actual deletion, as the expression is only
    // available as an argument to the function.
    Ok(value.get_by_path(path).cloned().unwrap_or(Value::Null))
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_exists_external(context: &mut Context, path: &LookupBuf) -> Resolved {
    Ok(context
        .target
        .target_get(path)
        .ok()
        .flatten()
        .is_some()
        .into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_exists_internal(variable: &Resolved, path: &LookupBuf) -> Resolved {
    let value = variable
        .as_ref()
        .expect("variable must not contain an error");
    Ok(value.get_by_path(path).is_some().into())
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_exists_expression(expression: Resolved, path: &LookupBuf) -> Resolved {
    let value = expression.expect("expression must not contain an error");
    // No need to do the actual deletion, as the expression is only
    // available as an argument to the function.
    Ok(value.get_by_path(path).is_some().into())
}
