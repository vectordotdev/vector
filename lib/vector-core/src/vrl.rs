use lookup::LookupBuf;
use vrl_lib::{diagnostic::DiagnosticList, state, Function, Program};

/// Compiles a VRL program
/// Vector metadata is set to read-only to prevent it from being mutated
///
/// # Errors
/// If the program fails to compile, a `DiagnosticList` of errors is returned
pub fn compile_vrl(
    source: &str,
    fns: &[Box<dyn Function>],
    external: &mut state::ExternalEnv,
    local: state::LocalEnv,
) -> Result<(Program, DiagnosticList), DiagnosticList> {
    // Prevent mutating anything under the "vector" path in metadata.
    //
    // This path is used to differentiate between log namespaces. It also contains
    // metadata that transforms / sinks may rely on, so setting it to read-only
    // prevents users from potentially breaking behavior relying on it.
    external.set_read_only_metadata_path(LookupBuf::from("vector"), true);

    vrl_lib::compile_with_state(source, fns, external, local)
}
