use std::collections::BTreeMap;
use vrl_core::{
    Context, ExpressionError, LookupBuf, Resolved, Span, Target, Value, VrlValueArithmetic,
    VrlValueConvert,
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
pub unsafe extern "C" fn vrl_resolved_initialize(result: *mut Resolved) {
    result.write(Ok(Value::Null));
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_resolved_drop(result: *mut Resolved) {
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
pub unsafe extern "C" fn vrl_btree_map_initialize(map: *mut BTreeMap<String, Value>) {
    map.write(Default::default());
}

/// # Safety
/// TODO.
#[no_mangle]
pub unsafe extern "C" fn vrl_btree_map_drop(map: *mut BTreeMap<String, Value>) {
    drop(map.read());
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
        (Err(_), _) => (),
        (_, Err(error)) => {
            *result = Err(error);
        }
    }
}

#[no_mangle]
pub extern "C" fn vrl_expression_op_eq_impl(rhs: &mut Resolved, result: &mut Resolved) {
    let lhs = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(result, &mut moved);
        moved
    }
    .expect("VRL value must not contain an error");
    let rhs = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(rhs, &mut moved);
        moved
    }
    .expect("VRL value must not contain an error");
    *result = Ok(lhs.eq_lossy(&rhs).into());
}

#[no_mangle]
pub extern "C" fn vrl_expression_op_eq_bytes_impl(rhs: &mut Resolved, result: &mut Resolved) {
    let lhs = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(result, &mut moved);
        moved
    }
    .expect("VRL value must not contain an error");
    let rhs = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(rhs, &mut moved);
        moved
    }
    .expect("VRL value must not contain an error");
    *result = Ok((lhs.as_bytes().expect("VRL value must be bytes")
        == rhs.as_bytes().expect("VRL value must be bytes"))
    .into());
}

#[no_mangle]
pub extern "C" fn vrl_expression_op_eq_integer_impl(rhs: &mut Resolved, result: &mut Resolved) {
    let lhs = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(result, &mut moved);
        moved
    }
    .expect("VRL value must not contain an error");
    let rhs = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(rhs, &mut moved);
        moved
    }
    .expect("VRL value must not contain an error");
    *result = Ok((lhs.as_integer().expect("VRL value must be bytes")
        == rhs.as_integer().expect("VRL value must be bytes"))
    .into());
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
        .map(|value| value.clone())
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
        .map(|value| value.clone())
        .unwrap_or(Value::Null));
}

#[no_mangle]
pub extern "C" fn vrl_fn_downcase(value: &mut Resolved, resolved: &mut Resolved) {
    let value = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(value, &mut moved);
        moved
    };

    *resolved = (|| {
        let bytes = value?.try_bytes()?;
        Ok(String::from_utf8_lossy(&bytes).to_lowercase().into())
    })();
}

#[no_mangle]
pub extern "C" fn vrl_fn_starts_with(
    value: &mut Resolved,
    substring: &mut Resolved,
    case_sensitive: &mut Resolved,
    resolved: &mut Resolved,
) {
    let value = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(value, &mut moved);
        moved
    };
    let substring = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(substring, &mut moved);
        moved
    };
    let case_sensitive = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(case_sensitive, &mut moved);
        moved
    };

    *resolved = (|| {
        let case_sensitive = case_sensitive?.try_boolean().unwrap_or(true);
        let substring = {
            let substring = substring?;
            let string = substring.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        let value = {
            let value = value?;
            let string = value.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        Ok(value.starts_with(&substring).into())
    })();
}

#[no_mangle]
pub extern "C" fn vrl_fn_string(value: &mut Resolved, resolved: &mut Resolved) {
    let value = {
        let mut moved = Ok(Value::Null);
        std::mem::swap(value, &mut moved);
        moved
    };

    *resolved = (|| match value? {
        v @ Value::Bytes(_) => Ok(v),
        v => Err(format!(r#"expected "string", got {}"#, v.kind()).into()),
    })();
}
