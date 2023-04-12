#![allow(dead_code)]

use serde::Serialize;
use vector_config::NamedComponent;

// Essentially, this just derives an impl of `vector_config::NamedComponent` where the component
// name comes from the attribute value i.e. the name of a component annotated with
// `#[component_name("weee")]` is "weee".
//
// We've run through the different possible combinations of generics, bounds, etc, to ensure that
// the macro is correctly rebuilding the output token stream.

#[derive(NamedComponent)]
#[source_component("basic")]
pub struct Basic;

#[derive(NamedComponent)]
#[transform_component("generics")]
pub struct Generics<T> {
    inner: T,
}

#[derive(NamedComponent)]
#[provider_component("bounds_abound")]
pub struct Bounds<T>
where
    T: AsRef<u64>,
{
    inner: T,
}

#[derive(NamedComponent)]
#[enrichment_table_component("existing_attrs")]
#[derive(Serialize)]
pub struct ExistingAttributes {
    foo: String,
}

fn assert_serialize<T: Serialize>() {}

fn verify_asserts() {
    assert_serialize::<ExistingAttributes>();
}
