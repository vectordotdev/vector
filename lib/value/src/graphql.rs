use async_graphql::scalar;

use crate::value::Value;

// For raw JSON data.
scalar!(Value, "Json", "Raw JSON data`");
