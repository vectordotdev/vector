use std::collections::{BTreeMap, HashMap};

use indexmap::IndexMap;
use vector_config::{configurable_component, schema::generate_root_schema, ConfigurableString};

/// A type that pretends to be `ConfigurableString` but has a non-string-like schema.
#[configurable_component]
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FakeString(u64);

impl ConfigurableString for FakeString {}

impl ToString for FakeString {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[test]
#[should_panic]
fn non_string_key_schema_stdlib_hashmap() {
    /// A HashMap-specific struct for testing fake string keys.
    #[derive(Clone)]
    #[configurable_component]
    pub struct SimpleHashMapTags {
        /// Some tags.
        tags: HashMap<FakeString, String>,
    }

    generate_root_schema::<SimpleHashMapTags>().unwrap();
}

#[test]
#[should_panic]
fn non_string_key_schema_stdlib_btreemap() {
    /// A BTreeMap-specific struct for testing fake string keys.
    #[derive(Clone)]
    #[configurable_component]
    pub struct SimpleBTreeMapTags {
        /// Some tags.
        tags: BTreeMap<FakeString, String>,
    }

    generate_root_schema::<SimpleBTreeMapTags>().unwrap();
}

#[test]
#[should_panic]
fn non_string_key_schema_stdlib_indexmap() {
    /// A IndexMap-specific struct for testing fake string keys.
    #[derive(Clone)]
    #[configurable_component]
    pub struct SimpleIndexMapTags {
        /// Some tags.
        tags: IndexMap<FakeString, String>,
    }

    generate_root_schema::<SimpleIndexMapTags>().unwrap();
}
