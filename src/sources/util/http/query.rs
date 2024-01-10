use std::collections::HashMap;

use vector_lib::lookup::path;
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    event::Event,
};

pub fn add_query_parameters(
    events: &mut [Event],
    query_parameters_config: &[String],
    query_parameters: &HashMap<String, String>,
    log_namespace: LogNamespace,
    source_name: &'static str,
) {
    for query_parameter_name in query_parameters_config {
        let value = query_parameters.get(query_parameter_name);
        for event in events.iter_mut() {
            if let Event::Log(log) = event {
                log_namespace.insert_source_metadata(
                    source_name,
                    log,
                    Some(LegacyKey::Overwrite(path!(query_parameter_name))),
                    path!("query_parameters"),
                    crate::event::Value::from(value.map(String::to_owned)),
                );
            }
        }
    }
}
