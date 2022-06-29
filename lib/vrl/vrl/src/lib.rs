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

use compiler::Compiler;
pub use compiler::{
    function, state, value, Context, Expression, Function, MetadataTarget, Program, ProgramInfo,
    SecretTarget, Target, TargetValue, TargetValueRef, VrlRuntime,
};
pub use diagnostic;
use lookup::LookupBuf;
pub use runtime::{Runtime, RuntimeResult, Terminate};
pub use vector_common::TimeZone;

/// Compile a given source into the final [`Program`].
pub fn compile(source: &str, fns: &[Box<dyn Function>]) -> compiler::Result {
    let mut state = state::ExternalEnv::default();

    compile_with_external(source, fns, &mut state)
}

pub fn compile_with_external(
    source: &str,
    fns: &[Box<dyn Function>],
    external: &mut state::ExternalEnv,
) -> compiler::Result {
    compile_with_state(source, fns, external, state::LocalEnv::default())
}

pub fn compile_with_state(
    source: &str,
    fns: &[Box<dyn Function>],
    external: &mut state::ExternalEnv,
    local: state::LocalEnv,
) -> compiler::Result {
    let ast = parser::parse(source)
        .map_err(|err| diagnostic::DiagnosticList::from(vec![Box::new(err) as Box<_>]))?;

    // Prevent mutating anything under the "vector" path in metadata. There are no cases
    // where this should be allowed in VRL.
    //
    // This path is used to differentiate between log namespaces. It also contains
    // metadata that transforms / sinks may rely on, so setting it to read-only
    // prevents users from potentially breaking behavior relying on it.
    external.add_read_only_metadata_path(LookupBuf::from("vector"), true);

    Compiler::compile(fns, ast, external, local)
}
