use lookup::LookupBuf;
use vrl_lib::state::TypeState;
use vrl_lib::{diagnostic::DiagnosticList, CompilationResult, CompileConfig, Function};

/// Compiles a VRL program
/// Vector metadata is set to read-only to prevent it from being mutated
///
/// # Errors
/// If the program fails to compile, a `DiagnosticList` of errors is returned
pub fn compile_vrl(
    source: &str,
    fns: &[Box<dyn Function>],
    state: &TypeState,
    config: CompileConfig,
) -> Result<CompilationResult, DiagnosticList> {
    let mut state = state.clone();

    // Prevent mutating anything under the "vector" path in metadata.
    //
    // This path is used to differentiate between log namespaces. It also contains
    // metadata that transforms / sinks may rely on, so setting it to read-only
    // prevents users from potentially breaking behavior relying on it.
    state
        .external
        .set_read_only_metadata_path(LookupBuf::from("vector"), true);

    vrl_lib::compile_with_state(source, fns, &state, config)
}
