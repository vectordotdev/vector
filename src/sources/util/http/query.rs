use std::collections::HashMap;

use vector_lib::lookup::path;
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    event::Event,
};

use crate::sources::http_server::HttpConfigParamKind;

pub fn add_query_parameters(
    events: &mut [Event],
    query_parameters_config: &[HttpConfigParamKind],
    query_parameters: &HashMap<String, String>,
    log_namespace: LogNamespace,
    source_name: &'static str,
) {
    for qp in query_parameters_config {
        match qp {
            // Add each non-wildcard containing query_parameter that was specified
            // in the `query_parameters` config option to the event if an exact match
            // is found.
            HttpConfigParamKind::Exact(query_parameter_name) => {
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
            // Add all query_parameters that match against wildcard pattens specified
            // in the `query_parameters` config option to the event.
            HttpConfigParamKind::Glob(query_parameter_pattern) => {
                for query_parameter_name in query_parameters.keys() {
                    if query_parameter_pattern
                        .matches_with(query_parameter_name.as_str(), glob::MatchOptions::default())
                    {
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
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::event::LogEvent;
    use crate::sources::{http_server::HttpConfigParamKind, util::add_query_parameters};

    use vector_lib::config::LogNamespace;
    use vrl::{path, value};

    #[test]
    fn multiple_query_params() {
        let query_params_names = [
            HttpConfigParamKind::Exact("param1".into()),
            HttpConfigParamKind::Exact("param2".into()),
        ];
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
    #[test]
    fn multiple_query_params_wildcard() {
        let query_params_names = [HttpConfigParamKind::Glob(glob::Pattern::new("*").unwrap())];
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

        let log = base_log[0].as_log();
        assert_eq!(
            log.value(),
            namespaced_log[0]
                .metadata()
                .value()
                .get(path!("test", "query_parameters"))
                .unwrap(),
            "Checking legacy and namespaced log contain query parameters string"
        );
        assert_eq!(
            log["param1"],
            "value1".into(),
            "Checking log contains first query parameter"
        );
        assert_eq!(
            log["param2"],
            "value2".into(),
            "Checking log contains second query parameter"
        );
        assert_eq!(
            log["param3"],
            "value3".into(),
            "Checking log contains third query parameter"
        );
    }
}
