use graphql_client::GraphQLQuery;
use serde_json::json;
use uuid::Uuid;

// Helper to merge two serde JSON values together. Used to augment subscription data.
// fn merge_json(a: &mut Value, b: Value) {
//     if let Value::Object(a) = a {
//         if let Value::Object(b) = b {
//             for (k, v) in b {
//                 if v.is_null() {
//                     a.remove(&k);
//                 } else {
//                     merge_json(a.entry(k).or_insert(Value::Null), v);
//                 }
//             }
//             return;
//         }
//     }
//     *a = b;
// }

/// Transforms a GraphQL QueryBody into a subscription request, by augmenting with the required
/// fields { type, id } and returning the JSON necessary to initialize a subscription `start`
pub fn make_subscription_request<T: GraphQLQuery>(
    request_body: &graphql_client::QueryBody<T::Variables>,
) -> serde_json::Value {
    json! ({
        "id":  Uuid::new_v4(),
        "type": "start",
        "payload": request_body
    })
}
