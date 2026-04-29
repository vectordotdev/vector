//! Compile-and-run check that `register_extra_metric_label!` is fully self-contained when
//! invoked through `vector-lib`. `vector-lib` does not declare `inventory` as a direct
//! dependency, so if this test compiles we know the macro does not leak the `::inventory` path
//! requirement onto downstream callers.

use vector_lib::metrics::MetricLabel;

vector_lib::register_extra_metric_label!("vector_lib_integration_extra_label");

#[test]
fn registered_label_is_reachable_through_vector_lib() {
    // Just assert the type is constructible and reachable through `vector_lib::metrics`. The
    // macro's actual side effect (population of the inventory registry) is exercised by
    // `vector-core`'s unit tests; this test is here to lock down the cross-crate invocation
    // shape.
    let registered = MetricLabel("vector_lib_integration_extra_label");
    assert_eq!(registered.0, "vector_lib_integration_extra_label");
}
