use serde_json::{Value, json};

use super::{
    EventStatus, LOGS, LogNamespace, OpentelemetryConfig, SourceConfig, SourceContext, new_source,
};
use crate::config::GenerateConfig;
use crate::sources::opentelemetry::config::OtlpDecodingConfig;
use vector_lib::codecs::decoding::OtlpSignalType;

#[allow(
    clippy::needless_pass_by_value,
    reason = "Changing this source helper signature is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
fn parse(options: Value) -> Result<OpentelemetryConfig, serde_json::Error> {
    let mut value = json!({
        "grpc": {"address": "127.0.0.1:0"},
        "http": {"address": "127.0.0.1:0"}
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(options.as_object().unwrap().clone());
    serde_json::from_value(value)
}

#[test]
fn defaults_and_legacy_booleans_preserve_behavior() {
    for (options, enabled) in [
        (json!({}), false),
        (json!({"use_otlp_decoding": false}), false),
        (json!({"use_otlp_decoding": true}), true),
        (json!({"decoding": {"codec": "otlp"}}), true),
        (
            json!({"decoding": {"codec": "otlp", "signal_types": []}}),
            false,
        ),
    ] {
        let config = parse(options).unwrap();
        assert_eq!(config.otlp_decoding(), OtlpDecodingConfig::from(enabled));
        let serialized = serde_json::to_value(&config).unwrap();
        let roundtrip: OpentelemetryConfig = serde_json::from_value(serialized).unwrap();
        assert_eq!(roundtrip.otlp_decoding(), config.otlp_decoding());
    }
    let generated = OpentelemetryConfig::generate_config();
    assert!(generated.get("use_otlp_decoding").is_none());
    assert!(generated.get("decoding").is_none());
}

#[test]
fn all_signal_selections_preserve_decoders_and_output_schemas() {
    let signals = [
        ("logs", OtlpSignalType::Logs),
        ("metrics", OtlpSignalType::Metrics),
        ("traces", OtlpSignalType::Traces),
    ];
    for mask in 0..8 {
        let selected: Vec<_> = signals
            .iter()
            .enumerate()
            .filter_map(|(i, (name, _))| (mask & (1 << i) != 0).then_some(*name))
            .collect();
        let legacy = parse(json!({"use_otlp_decoding": {"logs": mask & 1 != 0, "metrics": mask & 2 != 0, "traces": mask & 4 != 0}})).unwrap();
        let current =
            parse(json!({"decoding": {"codec": "otlp", "signal_types": selected}})).unwrap();
        assert_eq!(current.otlp_decoding(), legacy.otlp_decoding());
        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            for (old, new) in legacy
                .outputs(namespace)
                .iter()
                .zip(current.outputs(namespace))
            {
                assert_eq!(old.port, new.port);
                assert_eq!(old.ty, new.ty);
                assert_eq!(old.schema_definition, new.schema_definition);
            }
        }
        for (i, (_, signal)) in signals.iter().enumerate() {
            assert_eq!(
                current.get_signal_deserializer(*signal).unwrap().is_some(),
                mask & (1 << i) != 0
            );
        }
    }
    let repeated =
        parse(json!({"decoding": {"codec": "otlp", "signal_types": ["traces", "logs", "traces"]}}))
            .unwrap();
    assert_eq!(
        repeated.otlp_decoding(),
        OtlpDecodingConfig {
            logs: true,
            metrics: false,
            traces: true
        }
    );
}

#[test]
fn unsupported_codecs_and_invalid_options_are_rejected() {
    for codec in ["json", "native", "bytes", "protobuf", "invalid"] {
        let error = parse(json!({"decoding": {"codec": codec}}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown variant"), "{error}");
        assert!(error.contains("otlp"), "{error}");
    }
    for decoding in [
        json!({}),
        json!({"codec": "otlp", "signal_types": ["unknown"]}),
        json!({"codec": "otlp", "signal_types": true}),
        json!({"codec": "otlp", "signal_types": null}),
        json!({"codec": "otlp", "unexpected": true}),
        json!(true),
    ] {
        assert!(parse(json!({"decoding": decoding})).is_err());
    }
}

#[tokio::test]
async fn old_and_new_options_cannot_be_combined_even_when_disabled() {
    for legacy in [
        json!(false),
        json!(true),
        json!({}),
        json!({"traces": true}),
    ] {
        let config =
            parse(json!({"decoding": {"codec": "otlp"}, "use_otlp_decoding": legacy})).unwrap();
        let (sender, _, _) = new_source(EventStatus::Delivered, LOGS.to_string());
        let result = config.build(SourceContext::new_test(sender, None)).await;
        let Err(error) = result else {
            panic!("conflicting options were accepted")
        };
        assert_eq!(
            error.to_string(),
            "`decoding` and `use_otlp_decoding` cannot be set together"
        );
    }
}
