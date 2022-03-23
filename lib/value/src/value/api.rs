#![allow(clippy::use_self)] // I don't think Self can be used here, it creates a cycle

use async_graphql::scalar;

use crate::value::Value;

// For raw JSON data.
scalar!(Value, "Json", "Raw JSON data`");
