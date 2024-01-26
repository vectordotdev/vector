use metrics::{counter, gauge};
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL};

use crate::transforms::lua::v2::BuildError;

#[derive(Debug)]
pub struct LuaGcTriggered {
    pub used_memory: usize,
}

impl InternalEvent for LuaGcTriggered {
    fn emit(self) {
        gauge!("lua_memory_used_bytes", self.used_memory as f64);
    }
}

#[derive(Debug)]
pub struct LuaScriptError {
    pub error: mlua::Error,
}

impl InternalEvent for LuaScriptError {
    fn emit(self) {
        error!(
            message = "Error in lua script.",
            error = ?self.error,
            error_code = mlua_error_code(&self.error),
            error_type = error_type::COMMAND_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => mlua_error_code(&self.error),
            "error_type" => error_type::SCRIPT_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: "Error in lua script.",
        });
    }
}

#[derive(Debug)]
pub struct LuaBuildError {
    pub error: BuildError,
}

impl InternalEvent for LuaBuildError {
    fn emit(self) {
        let reason = "Error in building lua script.";
        error!(
            message = reason,
            error = ?self.error,
            error_type = error_type::SCRIPT_FAILED,
            error_code = lua_build_error_code(&self.error),
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => lua_build_error_code(&self.error),
            "error_type" => error_type::SCRIPT_FAILED,
            "stage" => error_stage:: PROCESSING,
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}

const fn mlua_error_code(err: &mlua::Error) -> &'static str {
    use mlua::Error::*;

    match err {
        SyntaxError { .. } => "syntax_error",
        RuntimeError(_) => "runtime_error",
        MemoryError(_) => "memory_error",
        SafetyError(_) => "memory_safety_error",
        MemoryLimitNotAvailable => "memory_limit_not_available",
        RecursiveMutCallback => "mutable_callback_called_recursively",
        CallbackDestructed => "callback_destructed",
        StackError => "out_of_stack",
        BindError => "too_many_arguments_to_function_bind",
        ToLuaConversionError { .. } => "error_converting_value_to_lua",
        FromLuaConversionError { .. } => "error_converting_value_from_lua",
        CoroutineInactive => "coroutine_inactive",
        UserDataTypeMismatch => "userdata_type_mismatch",
        UserDataDestructed => "userdata_destructed",
        UserDataBorrowError => "userdata_borrow_error",
        UserDataBorrowMutError => "userdata_already_borrowed",
        MetaMethodRestricted(_) => "restricted_metamethod",
        MetaMethodTypeError { .. } => "unsupported_metamethod_type",
        MismatchedRegistryKey => "mismatched_registry_key",
        CallbackError { .. } => "callback_error",
        PreviouslyResumedPanic => "previously_resumed_panic",
        ExternalError(_) => "external_error",
        _ => "unknown",
    }
}

const fn lua_build_error_code(err: &BuildError) -> &'static str {
    use BuildError::*;

    match err {
        InvalidSearchDirs { .. } => "invalid_search_dir",
        InvalidSource { .. } => "invalid_source",
        InvalidHooksInit { .. } => "invalid_hook_init",
        InvalidHooksProcess { .. } => "invalid_hook_process",
        InvalidHooksShutdown { .. } => "invalid_hook_shutdown",
        InvalidTimerHandler { .. } => "invalid_timer_handler",
        RuntimeErrorHooksInit { .. } => "runtime_error_hook_init",
        RuntimeErrorHooksProcess { .. } => "runtime_error_hook_process",
        RuntimeErrorHooksShutdown { .. } => "runtime_error_hook_shutdown",
        RuntimeErrorTimerHandler { .. } => "runtime_error_timer_handler",
        RuntimeErrorGc { .. } => "runtime_error_gc",
    }
}
