use crate::event::Value;
use async_graphql::scalar;

// For raw JSON data.
scalar!(Value, "Json", "Raw JSON data`");
