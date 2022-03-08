use std::collections::HashMap;

use vector_core::event::Event;

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
