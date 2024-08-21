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
                    path!("query_parameters", query_parameter_name),
                    crate::event::Value::from(value.map(String::to_owned)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::event::LogEvent;
    use crate::sources::util::add_query_parameters;
    use vector_lib::config::LogNamespace;
    use vrl::{path, value};

    #[test]
    fn multiple_query_params() {
        let query_params_names = ["param1".into(), "param2".into()];
        let query_params = [
            ("param1".into(), "value1".into()),
            ("param2".into(), "value2".into()),
            ("param3".into(), "value3".into()),
        ]
        .into();

        let mut base_log = [LogEvent::from(value!({})).into()];
        add_query_parameters(
            &mut base_log,
            &query_params_names,
            &query_params,
            LogNamespace::Legacy,
            "test",
        );
        let mut namespaced_log = [LogEvent::from(value!({})).into()];
        add_query_parameters(
            &mut namespaced_log,
            &query_params_names,
            &query_params,
            LogNamespace::Vector,
            "test",
        );

        assert_eq!(
            base_log[0].as_log().value(),
            namespaced_log[0]
                .metadata()
                .value()
                .get(path!("test", "query_parameters"))
                .unwrap()
        );
    }
}
