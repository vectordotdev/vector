use async_graphql::scalar;

use crate::event::Value;

// For raw JSON data.
scalar!(Value, "Json", "Raw JSON data`");
