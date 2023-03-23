#![deny(warnings)]
#![deny(clippy::all)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]
#![allow(clippy::module_name_repetitions)]

pub mod prelude;
mod runtime;

pub use compiler::{
    function, state, value, CompilationResult, CompileConfig, Compiler, Context, Expression,
    Function, Program, ProgramInfo, SecretTarget, Target, TargetValue, TargetValueRef, VrlRuntime,
};
pub use diagnostic;
pub use runtime::{Runtime, RuntimeResult, Terminate};
pub use vrl_core::TimeZone;

use crate::state::TypeState;
pub use compiler::expression::query;

/// Compile a given source into the final [`Program`].
pub fn compile(source: &str, fns: &[Box<dyn Function>]) -> compiler::Result {
    let external = state::ExternalEnv::default();
    let config = CompileConfig::default();

    compile_with_external(source, fns, &external, config)
}

pub fn compile_with_external(
    source: &str,
    fns: &[Box<dyn Function>],
    external: &state::ExternalEnv,
    config: CompileConfig,
) -> compiler::Result {
    let state = TypeState {
        local: state::LocalEnv::default(),
        external: external.clone(),
    };

    compile_with_state(source, fns, &state, config)
}

pub fn compile_with_state(
    source: &str,
    fns: &[Box<dyn Function>],
    state: &TypeState,
    config: CompileConfig,
) -> compiler::Result {
    let ast = parser::parse(source)
        .map_err(|err| diagnostic::DiagnosticList::from(vec![Box::new(err) as Box<_>]))?;

    Compiler::compile(fns, ast, state, config)
}
