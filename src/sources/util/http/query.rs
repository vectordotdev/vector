use std::collections::HashMap;

use vector_core::event::Event;

// TODO when the `heroku` log namespacing work is undertaken, replace this function with the one from
// sources::http_server::add_query_parameters() and remove that function from http_server.
// (https://github.com/vectordotdev/vector/issues/15022)
pub fn add_query_parameters(
    events: &mut [Event],
    query_parameters_config: &[String],
    query_parameters: HashMap<String, String>,
) {
    for query_parameter_name in query_parameters_config {
        let value = query_parameters.get(query_parameter_name);
        for event in events.iter_mut() {
            event.as_mut_log().insert(
                query_parameter_name as &str,
                crate::event::Value::from(value.map(String::to_owned)),
            );
        }
    }
}
