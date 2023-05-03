use lookup::{owned_value_path, OwnedTargetPath};
use vrl::compiler::{compile_with_state, CompilationResult, CompileConfig, Function, TypeState};
use vrl::diagnostic::DiagnosticList;

/// Compiles a VRL program
/// Vector metadata is set to read-only to prevent it from being mutated
///
/// # Errors
/// If the program fails to compile, a `DiagnosticList` of errors is returned
pub fn compile_vrl(
    source: &str,
    fns: &[Box<dyn Function>],
    state: &TypeState,
    mut config: CompileConfig,
) -> Result<CompilationResult, DiagnosticList> {
    // Prevent mutating anything under the "vector" path in metadata.
    //
    // This path is used to differentiate between log namespaces. It also contains
    // metadata that transforms / sinks may rely on, so setting it to read-only
    // prevents users from potentially breaking behavior relying on it.
    config.set_read_only_path(OwnedTargetPath::metadata(owned_value_path!("vector")), true);

    compile_with_state(source, fns, state, config)
}
