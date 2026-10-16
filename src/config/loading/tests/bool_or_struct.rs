use serde_json::{Value, json};
use vector_config::schema::generate_root_schema;

use crate::config::{ConfigBuilder, loading::schema_coercion::ValueCoercer};

#[test]
fn existing_boolean_shorthands_match_generated_schema() {
    let schema = serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
    let cases = [
        (
            "global acknowledgements",
            json!({"acknowledgements": null}),
            "/acknowledgements",
            false,
        ),
        #[cfg(feature = "sinks-console")]
        (
            "sink acknowledgements",
            json!({"sinks": {"out": {
                "type": "console", "inputs": [], "encoding": {"codec": "json"}, "acknowledgements": null
            }}}),
            "/sinks/out/acknowledgements",
            false,
        ),
        #[cfg(feature = "sinks-console")]
        (
            "healthcheck",
            json!({"sinks": {"out": {
                "type": "console", "inputs": [], "encoding": {"codec": "json"}, "healthcheck": null
            }}}),
            "/sinks/out/healthcheck",
            false,
        ),
        #[cfg(feature = "sinks-nats")]
        (
            "JetStream",
            json!({"sinks": {"out": {
                "type": "nats", "inputs": [], "encoding": {"codec": "json"},
                "url": "nats://127.0.0.1:4222", "subject": "events", "jetstream": null
            }}}),
            "/sinks/out/jetstream",
            false,
        ),
        #[cfg(feature = "sources-opentelemetry")]
        (
            "source acknowledgements",
            json!({"sources": {"in": {
                "type": "opentelemetry", "grpc": {"address": "127.0.0.1:4317"},
                "http": {"address": "127.0.0.1:4318"}, "acknowledgements": null
            }}}),
            "/sources/in/acknowledgements",
            false,
        ),
        #[cfg(feature = "sources-opentelemetry")]
        (
            "OTLP decoding",
            json!({"sources": {"in": {
                "type": "opentelemetry", "grpc": {"address": "127.0.0.1:4317"},
                "http": {"address": "127.0.0.1:4318"}, "use_otlp_decoding": null
            }}}),
            "/sources/in/use_otlp_decoding",
            true,
        ),
        #[cfg(feature = "sources-datadog_agent")]
        (
            "Datadog source acknowledgements",
            json!({"sources": {"in": {
                "type": "datadog_agent", "address": "127.0.0.1:8080", "acknowledgements": null
            }}}),
            "/sources/in/acknowledgements",
            false,
        ),
        #[cfg(feature = "sources-splunk_hec")]
        (
            "Splunk source acknowledgements",
            json!({"sources": {"in": {
                "type": "splunk_hec", "acknowledgements": null
            }}}),
            "/sources/in/acknowledgements",
            false,
        ),
    ];
    for (name, template, pointer, all_signals) in cases {
        assert_shorthand_matches_schema(&schema, name, &template, pointer, all_signals);
    }
}

fn assert_shorthand_matches_schema(
    schema: &Value,
    name: &str,
    template: &Value,
    pointer: &str,
    all_signals: bool,
) {
    for enabled in [false, true] {
        let mut input = template.clone();
        *input.pointer_mut(pointer).unwrap() = Value::Bool(enabled);
        let mut object_input = input.clone();
        *object_input.pointer_mut(pointer).unwrap() = if all_signals {
            json!({"logs": enabled, "metrics": enabled, "traces": enabled})
        } else {
            json!({"enabled": enabled})
        };
        let shorthand: ConfigBuilder =
            serde_json::from_value(input.clone()).unwrap_or_else(|error| panic!("{name}: {error}"));
        let object: ConfigBuilder =
            serde_json::from_value(object_input).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            serde_json::to_value(shorthand).unwrap(),
            serde_json::to_value(object).unwrap(),
            "{name}"
        );

        let mut coerced = input.clone();
        ValueCoercer::new(schema)
            .coerce(&mut coerced)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(coerced, input, "{name}");
    }
}
