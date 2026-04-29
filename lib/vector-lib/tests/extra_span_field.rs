//! Compile-and-run check that `register_extra_span_field!` is fully self-contained when
//! invoked through `vector-lib`. `vector-lib` does not declare `inventory` as a direct
//! dependency, so if this test compiles we know the macro does not leak the `::inventory` path
//! requirement onto downstream callers.

use vector_lib::{SpanField, metrics::MetricLabel};

vector_lib::register_extra_span_field!("vector_lib_integration_extra_field");

#[test]
fn registered_field_is_reachable_through_vector_lib() {
    // Both inventory item types are reachable through `vector_lib`, which is what callers
    // will touch in practice. The macro's actual side effect — population of the metrics and
    // log inventories — is exercised by the unit tests in `vector-core` and `vector::trace`;
    // this test only locks down the cross-crate invocation shape.
    let metric_entry = MetricLabel("vector_lib_integration_extra_field");
    let log_entry = SpanField("vector_lib_integration_extra_field");
    assert_eq!(metric_entry.0, "vector_lib_integration_extra_field");
    assert_eq!(log_entry.0, "vector_lib_integration_extra_field");
}
