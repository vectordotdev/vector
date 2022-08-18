#![allow(dead_code)]

use serde::Serialize;
use vector_config::component_name;

// Essentially, this just derives an impl of `vector_config::NamedComponent` where the component
// name comes from the attribute value i.e. the name of a component annotated with
// `#[component_name("weee")]` is "weee".
//
// We've run through the different possible combinations of generics, bounds, etc, to ensure that
// the macro is correctly rebuilding the output token stream.

#[component_name("basic")]
pub struct Basic;

#[component_name("generics")]
pub struct Generics<T> {
    inner: T,
}

#[component_name("bounds")]
pub struct Bounds<T>
where
    T: AsRef<u64>,
{
    inner: T,
}

#[component_name("existing_attrs")]
#[derive(Serialize)]
pub struct ExistingAttributes {
    foo: String,
}

fn assert_serialize<T: Serialize>() {}

fn verify_asserts() {
    assert_serialize::<ExistingAttributes>();
}
