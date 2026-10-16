use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use bollard::{
    Docker,
    query_parameters::{ListContainersOptionsBuilder, StopContainerOptionsBuilder},
};
use prost::Message;
use vector_lib::opentelemetry::proto::{
    collector::trace::v1::ExportTraceServiceRequest,
    common::v1::{AnyValue, KeyValue, any_value::Value},
    trace::v1::{ResourceSpans, ScopeSpans},
};

use crate::opentelemetry::parse_export_traces_request;

const OUTPUT: &str = "/output/opentelemetry-traces-multiservice";
const NETWORK: &str = "vector-integration-tests-opentelemetry-traces-multiservice";
const REQUIRED_SERVICES: &[&str] = &[
    "ad",
    "cart",
    "checkout",
    "currency",
    "email",
    "frontend",
    "payment",
    "product-catalog",
    "quote",
    "recommendation",
    "shipping",
    "load-generator",
    "accounting",
    "fraud-detection",
];

// Attribute lists are maps; arrays, events and links keep their meaningful order.
fn sort_attributes(attributes: &mut [KeyValue]) {
    fn sort_value(value: &mut AnyValue) {
        match value.value.as_mut() {
            Some(Value::KvlistValue(list)) => sort_attributes(&mut list.values),
            Some(Value::ArrayValue(array)) => array.values.iter_mut().for_each(sort_value),
            _ => {}
        }
    }
    for attribute in attributes.iter_mut() {
        if let Some(value) = &mut attribute.value {
            sort_value(value);
        }
    }
    attributes.sort_by(|a, b| a.key.cmp(&b.key));
}

type SpanKey = (Vec<u8>, Vec<u8>);

fn records(request: &ExportTraceServiceRequest) -> BTreeMap<SpanKey, Vec<Vec<u8>>> {
    let mut records: BTreeMap<SpanKey, Vec<Vec<u8>>> = BTreeMap::new();
    for rs in &request.resource_spans {
        for ss in &rs.scope_spans {
            for span in &ss.spans {
                let mut record = ResourceSpans {
                    resource: rs.resource.clone(),
                    schema_url: rs.schema_url.clone(),
                    scope_spans: vec![ScopeSpans {
                        scope: ss.scope.clone(),
                        schema_url: ss.schema_url.clone(),
                        spans: vec![span.clone()],
                    }],
                };
                if let Some(resource) = &mut record.resource {
                    sort_attributes(&mut resource.attributes);
                }
                let scope = &mut record.scope_spans[0];
                if let Some(scope) = &mut scope.scope {
                    sort_attributes(&mut scope.attributes);
                }
                let span = &mut scope.spans[0];
                sort_attributes(&mut span.attributes);
                for event in &mut span.events {
                    sort_attributes(&mut event.attributes);
                }
                for link in &mut span.links {
                    sort_attributes(&mut link.attributes);
                }
                records
                    .entry((span.trace_id.clone(), span.span_id.clone()))
                    .or_default()
                    .push(record.encode_to_vec());
            }
        }
    }
    for occurrences in records.values_mut() {
        occurrences.sort();
    }
    records
}

fn compare(source: &ExportTraceServiceRequest, sink: &ExportTraceServiceRequest) -> Vec<String> {
    let source = records(source);
    let sink = records(sink);
    let keys: BTreeSet<_> = source.keys().chain(sink.keys()).collect();
    keys.into_iter()
        .filter_map(|key| {
            let expected = source.get(key).cloned().unwrap_or_default();
            let actual = sink.get(key).cloned().unwrap_or_default();
            if expected == actual {
                return None;
            }
            let decode = |values: Vec<Vec<u8>>| {
                values
                    .into_iter()
                    .map(|value| ResourceSpans::decode(value.as_slice()).unwrap())
                    .collect::<Vec<_>>()
            };
            Some(format!(
                "trace={} span={}\nexpected: {:#?}\nactual: {:#?}",
                hex::encode(&key.0),
                hex::encode(&key.1),
                decode(expected),
                decode(actual),
            ))
        })
        .collect()
}

fn service(rs: &ResourceSpans) -> &str {
    rs.resource
        .as_ref()
        .and_then(|resource| {
            resource
                .attributes
                .iter()
                .find(|attribute| attribute.key == "service.name")
        })
        .and_then(|attribute| attribute.value.as_ref())
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match value {
            Value::StringValue(value) => Some(value.as_str()),
            _ => None,
        })
        .unwrap_or("<missing>")
}

fn coverage(request: &ExportTraceServiceRequest) -> serde_json::Value {
    let mut services = BTreeMap::<&str, usize>::new();
    let mut scopes = BTreeMap::<&str, usize>::new();
    let mut traces = BTreeMap::<&[u8], usize>::new();
    let mut parents = BTreeMap::new();
    let mut roots = 0;
    let mut errors = 0;
    let mut events = 0;
    let mut links = 0;
    let mut mixed_scopes = 0;
    for rs in &request.resource_spans {
        for ss in &rs.scope_spans {
            let ids: BTreeSet<_> = ss.spans.iter().map(|span| &span.trace_id).collect();
            mixed_scopes += usize::from(ids.len() > 1);
            for span in &ss.spans {
                *services.entry(service(rs)).or_default() += 1;
                *scopes
                    .entry(
                        ss.scope
                            .as_ref()
                            .map_or("<missing>", |scope| scope.name.as_str()),
                    )
                    .or_default() += 1;
                *traces.entry(&span.trace_id).or_default() += 1;
                parents.insert(
                    (span.trace_id.as_slice(), span.span_id.as_slice()),
                    service(rs),
                );
                roots += usize::from(
                    span.parent_span_id.is_empty()
                        || span.parent_span_id.iter().all(|byte| *byte == 0),
                );
                errors += usize::from(span.status.as_ref().is_some_and(|status| status.code == 2));
                events += span.events.len();
                links += span.links.len();
            }
        }
    }
    let mut children = 0;
    let mut cross_service = 0;
    for rs in &request.resource_spans {
        for ss in &rs.scope_spans {
            for span in &ss.spans {
                if let Some(parent_service) =
                    parents.get(&(span.trace_id.as_slice(), span.parent_span_id.as_slice()))
                {
                    children += 1;
                    cross_service += usize::from(*parent_service != service(rs));
                }
            }
        }
    }
    serde_json::json!({
        "services": services, "scopes": scopes,
        "spans": services.values().sum::<usize>(), "traces": traces.len(),
        "multi_span_traces": traces.values().filter(|count| **count > 1).count(),
        "root_spans": roots, "children_with_captured_parent": children,
        "cross_service_parent_relationships": cross_service,
        "error_spans": errors, "events": events, "links": links,
        "scope_groups_with_multiple_trace_ids": mixed_scopes,
    })
}

async fn stop_containers(docker: &Docker, filter: &str) -> Result<(), String> {
    let filters = HashMap::from([
        ("network".to_owned(), vec![NETWORK.to_owned()]),
        ("label".to_owned(), vec![filter.to_owned()]),
    ]);
    let containers = docker
        .list_containers(Some(
            ListContainersOptionsBuilder::new()
                .filters(&filters)
                .build(),
        ))
        .await
        .map_err(|error| error.to_string())?;
    if containers.is_empty() {
        return Err(format!("No running demo containers matching {filter}"));
    }
    // Stop producers together so consumers remain alive during SDK shutdown.
    futures::future::try_join_all(containers.iter().map(|container| async {
        docker
            .stop_container(
                container.id.as_deref().ok_or("Container ID missing")?,
                Some(StopContainerOptionsBuilder::new().t(30).build()),
            )
            .await
            .map_err(|error| error.to_string())
    }))
    .await?;
    Ok(())
}

fn read_capture(directory: &Path, name: &str) -> Result<ExportTraceServiceRequest, String> {
    let path = directory.join(name);
    parse_export_traces_request(
        &fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?,
    )
}

async fn validate_multiservice_traces() -> Result<(), String> {
    let exporter = std::env::var("CONFIG_INGRESS_EXPORTER").map_err(|error| error.to_string())?;
    let transport = match exporter.as_str() {
        "otlphttp" => "HTTP",
        "otlp" => "gRPC",
        _ => return Err(format!("Unsupported ingress exporter: {exporter}")),
    };
    let directory = Path::new(OUTPUT).join(&exporter);
    if !directory.join("workload-complete").exists() {
        return Err(
            "Demo workload did not complete; run cargo vdev e2e run opentelemetry-traces-multiservice".into(),
        );
    }
    let docker = Docker::connect_with_socket_defaults().map_err(|error| error.to_string())?;
    // Give asynchronous Kafka consumers time to process the final checkout.
    thread::sleep(Duration::from_secs(5));
    stop_containers(&docker, "vector.otel-traces-multiservice.producer=true").await?;
    stop_containers(&docker, "com.docker.compose.service=otel-collector").await?;
    let input = read_capture(&directory, "input.jsonl")?;
    let input_coverage = coverage(&input);
    fs::write(
        directory.join("coverage.json"),
        serde_json::to_string_pretty(&input_coverage).unwrap(),
    )
    .map_err(|error| error.to_string())?;

    let deadline = Instant::now() + Duration::from_secs(120);
    let mut failures = Vec::new();
    loop {
        if let Ok(output) = read_capture(&directory, "output.jsonl")
            && coverage(&output)["spans"].as_u64() >= input_coverage["spans"].as_u64()
        {
            break;
        }
        if Instant::now() >= deadline {
            failures.push("Timed out draining Vector; inspect input.jsonl and output.jsonl".into());
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    stop_containers(&docker, "com.docker.compose.service=otel-collector-sink").await?;
    let output = read_capture(&directory, "output.jsonl")?;
    let differences = compare(&input, &output);
    fs::write(directory.join("differences.txt"), differences.join("\n\n"))
        .map_err(|error| error.to_string())?;
    for name in REQUIRED_SERVICES {
        if input_coverage["services"][*name].as_u64().unwrap_or(0) == 0 {
            failures.push(format!("Missing service: {name}"));
        }
    }
    for field in [
        "root_spans",
        "multi_span_traces",
        "children_with_captured_parent",
        "cross_service_parent_relationships",
        "error_spans",
    ] {
        if input_coverage[field].as_u64().unwrap_or(0) == 0 {
            failures.push(format!("Missing trace shape: {field}"));
        }
    }
    if !differences.is_empty() {
        failures.push(format!(
            "{} differing span identities; see differences.txt",
            differences.len()
        ));
    }
    let summary = format!(
        "# Multiservice OpenTelemetry trace validation\n\nDemo: 3.1.0; Vector ingress: {transport}\n\n```json\n{}\n```\n\n{}\n",
        serde_json::to_string_pretty(&input_coverage).unwrap(),
        if failures.is_empty() {
            "PASS: captured trace semantics and multiplicities match.".into()
        } else {
            failures.join("\n")
        }
    );
    fs::write(directory.join("summary.md"), summary).map_err(|error| error.to_string())?;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[tokio::test]
async fn trace_round_trip() {
    validate_multiservice_traces()
        .await
        .unwrap_or_else(|error| panic!("{error}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_lib::opentelemetry::proto::trace::v1::{ResourceSpans, ScopeSpans, Span};

    fn request() -> ExportTraceServiceRequest {
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                scope_spans: vec![ScopeSpans {
                    spans: vec![Span {
                        trace_id: vec![1; 16],
                        span_id: vec![2; 8],
                        parent_span_id: vec![3; 8],
                        name: "checkout".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        }
    }

    #[test]
    fn comparison_preserves_multiplicity_and_context() {
        let source = request();
        let mut repeated = source.clone();
        repeated
            .resource_spans
            .push(source.resource_spans[0].clone());
        let mut rebatched = source.clone();
        rebatched.resource_spans[0].scope_spans[0]
            .spans
            .push(source.resource_spans[0].scope_spans[0].spans[0].clone());
        assert!(compare(&repeated, &rebatched).is_empty());
        assert!(!compare(&source, &rebatched).is_empty());
        assert!(!compare(&source, &ExportTraceServiceRequest::default()).is_empty());
        for field in ["parent", "span", "resource", "scope", "schema"] {
            let mut changed = source.clone();
            let rs = &mut changed.resource_spans[0];
            match field {
                "parent" => rs.scope_spans[0].spans[0].parent_span_id = vec![4; 8],
                "span" => rs.scope_spans[0].spans[0].name = "changed".into(),
                "resource" => rs.resource = Some(Default::default()),
                "scope" => rs.scope_spans[0].scope = Some(Default::default()),
                "schema" => rs.schema_url = "changed".into(),
                _ => unreachable!(),
            }
            assert!(!compare(&source, &changed).is_empty(), "missed {field}");
        }
    }

    #[test]
    fn comparison_ignores_attribute_order_but_preserves_values_and_events() {
        use vector_lib::opentelemetry::proto::{
            common::v1::KeyValueList,
            trace::v1::span::{Event, Link},
        };
        let attribute = |key: &str, value: &str| KeyValue {
            key: key.into(),
            value: Some(AnyValue {
                value: Some(Value::StringValue(value.into())),
            }),
        };
        let mut source = request();
        let span = &mut source.resource_spans[0].scope_spans[0].spans[0];
        span.attributes = vec![
            attribute("z", "last"),
            KeyValue {
                key: "nested".into(),
                value: Some(AnyValue {
                    value: Some(Value::KvlistValue(KeyValueList {
                        values: vec![attribute("b", "2"), attribute("a", "1")],
                    })),
                }),
            },
        ];
        span.events = vec![Event {
            name: "exception".into(),
            ..Default::default()
        }];
        span.links = vec![Link {
            trace_id: vec![4; 16],
            span_id: vec![5; 8],
            ..Default::default()
        }];
        let mut reordered = source.clone();
        let attributes = &mut reordered.resource_spans[0].scope_spans[0].spans[0].attributes;
        attributes.reverse();
        if let Some(Value::KvlistValue(list)) = attributes[0].value.as_mut().unwrap().value.as_mut()
        {
            list.values.reverse();
        }
        assert!(compare(&source, &reordered).is_empty());
        for field in ["attribute", "event", "link", "flags", "dropped"] {
            let mut changed = source.clone();
            let span = &mut changed.resource_spans[0].scope_spans[0].spans[0];
            match field {
                "attribute" => span.attributes[0] = attribute("z", "changed"),
                "event" => span.events[0].name = "changed".into(),
                "link" => span.links[0].span_id = vec![6; 8],
                "flags" => span.flags = 1,
                "dropped" => span.dropped_attributes_count = 1,
                _ => unreachable!(),
            }
            assert!(!compare(&source, &changed).is_empty(), "missed {field}");
        }
    }
}
