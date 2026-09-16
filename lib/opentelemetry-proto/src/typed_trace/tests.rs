use std::time::Duration;

use bytes::Bytes;
use chrono::DateTime;
use proptest::prelude::*;
use similar_asserts::assert_eq;
use vector_core::event::{
    BatchNotifier, BatchStatus, EventMetadata,
    typed_trace::{
        AttrValue, Attributes, DatadogAgentEnvelope, DatadogChunkContext, DatadogSpanContext,
        DroppedCount, OtlpSpanFlags, PanicOnIssue, Resource, SamplingPriority, Scope, Span,
        SpanEvent, SpanLink, SpanStatus, TraceConversionCounts, TraceEvent,
    },
};

use super::{
    DATADOG_AGENT, DATADOG_CHUNK_DROPPED, DATADOG_CHUNK_ORIGIN, DATADOG_CHUNK_PRIORITY,
    DATADOG_CHUNK_TAGS, DATADOG_SPAN_META_STRUCT, DATADOG_SPAN_RESOURCE, DATADOG_SPAN_TYPE,
    DATADOG_TRACER_TAGS, ENVIRONMENT_LEGACY, ENVIRONMENT_NAME, HOST_NAME, SERVICE_NAME,
    keys::is_reserved_datadog_key,
};
use crate::proto::{
    collector::trace::v1::ExportTraceServiceRequest,
    common::v1::{
        AnyValue, ArrayValue, InstrumentationScope, KeyValue, any_value::Value as PbValue,
    },
    resource::v1::Resource as PbResource,
    trace::v1::{
        ResourceSpans, ScopeSpans, Span as PbSpan, Status as PbStatus,
        span::{Event as PbEvent, Link as PbLink},
    },
};

use super::test_support::{
    attr_map, attrs, rejected, resource_event, sid, tid, typed_event, typed_event_with_metadata,
    typed_link, typed_span,
};

fn convert_spans(spans: Vec<PbSpan>) -> Vec<TraceEvent> {
    ResourceSpans::from_spans(spans).convert()
}

fn convert_span(span: PbSpan) -> TraceEvent {
    convert_spans(vec![span]).into_iter().next().unwrap()
}

fn encode_counted(events: &[TraceEvent]) -> (Vec<ResourceSpans>, TraceConversionCounts) {
    let mut counts = TraceConversionCounts::default();
    let (request, _finalizers) =
        ExportTraceServiceRequest::from_events(events.to_vec(), &mut counts);
    (request.resource_spans, counts)
}

fn encode(events: &[TraceEvent]) -> Vec<ResourceSpans> {
    ExportTraceServiceRequest::from_events(events.to_vec(), &mut PanicOnIssue)
        .0
        .resource_spans
}

#[test]
fn partitions_scope_spans_by_first_seen_trace_id() {
    let events = convert_spans(vec![
        PbSpan::from_parts(1, 10, "a1"),
        PbSpan::from_parts(2, 20, "b1"),
        PbSpan::from_parts(1, 11, "a2"),
        PbSpan::from_parts(3, 30, "c1"),
        PbSpan::from_parts(2, 21, "b2"),
    ]);
    assert_eq!(
        events,
        vec![
            typed_event(1, vec![typed_span(10, "a1"), typed_span(11, "a2")]),
            typed_event(2, vec![typed_span(20, "b1"), typed_span(21, "b2")]),
            typed_event(3, vec![typed_span(30, "c1")]),
        ]
    );
}

#[test]
fn empty_groupings_produce_no_event_and_are_reported() {
    let empty_scope = ResourceSpans::from_parts(
        Vec::new(),
        vec![KeyValue::from_string("service.name", "svc")],
    );
    let no_scope = ResourceSpans {
        scope_spans: Vec::new(),
        ..empty_scope.clone()
    };
    for rs in [empty_scope, no_scope] {
        assert_eq!(
            rs.decode(),
            (
                Vec::new(),
                TraceConversionCounts {
                    discarded_groupings: 1,
                    ..TraceConversionCounts::default()
                },
            )
        );
    }
}

#[test]
fn all_rejected_spans_produce_no_event() {
    let span = PbSpan::from_parts(1, 1, "bad");
    for (case, bad) in [
        (
            "zero trace ID",
            PbSpan {
                trace_id: vec![0; 16],
                ..span.clone()
            },
        ),
        (
            "short span ID",
            PbSpan {
                span_id: vec![1, 2, 3],
                ..span.clone()
            },
        ),
        (
            "short parent span ID",
            PbSpan {
                parent_span_id: vec![1, 2, 3],
                ..span
            },
        ),
    ] {
        assert_eq!(
            ResourceSpans::from_spans(vec![bad]).decode(),
            rejected(1),
            "{case}"
        );
    }
}

#[test]
fn zero_parent_is_none() {
    let root = PbSpan {
        parent_span_id: vec![0; 8],
        ..PbSpan::from_parts(1, 1, "root")
    };
    assert_eq!(
        convert_spans(vec![root]),
        vec![typed_event(1, vec![typed_span(1, "root")])]
    );
}

#[test]
fn rejected_spans_are_counted_and_siblings_survive() {
    let bad_trace = PbSpan {
        trace_id: vec![0; 16],
        ..PbSpan::from_parts(1, 1, "bad")
    };
    let bad_parent = PbSpan {
        parent_span_id: vec![1, 2, 3],
        ..PbSpan::from_parts(1, 2, "bad")
    };
    let out = ResourceSpans::from_spans(vec![
        bad_trace,
        PbSpan::from_parts(1, 3, "good"),
        bad_parent,
    ])
    .decode();
    assert_eq!(
        out,
        (
            vec![typed_event(1, vec![typed_span(3, "good")])],
            TraceConversionCounts {
                rejected_spans: 2,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn invalid_links_are_dropped_and_counted_in_band() {
    let span = PbSpan {
        dropped_links_count: 2,
        links: vec![
            PbLink::from_parts(2, 2),
            PbLink {
                trace_id: vec![0; 16],
                ..PbLink::from_parts(3, 3)
            },
            PbLink {
                span_id: vec![1],
                ..PbLink::from_parts(4, 4)
            },
        ],
        ..PbSpan::from_parts(1, 1, "s")
    };
    let out = ResourceSpans::from_spans(vec![span]).decode();
    assert_eq!(
        out,
        (
            vec![typed_event(
                1,
                vec![Span {
                    links: vec![typed_link(2, 2)],
                    dropped_links_count: DroppedCount::new(4),
                    ..typed_span(1, "s")
                }],
            )],
            TraceConversionCounts {
                dropped_links: 2,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn span_and_link_flags_round_trip() {
    let rs = ResourceSpans::from_spans(vec![PbSpan {
        flags: 0x0000_0301,
        links: vec![PbLink {
            flags: 0x8000_0100,
            ..PbLink::from_parts(2, 2)
        }],
        ..PbSpan::from_parts(1, 1, "s")
    }]);
    let events = rs.clone().convert();
    assert_eq!(
        events,
        vec![typed_event(
            1,
            vec![Span {
                flags: OtlpSpanFlags::from(0x0000_0301),
                links: vec![SpanLink {
                    flags: OtlpSpanFlags::from(0x8000_0100),
                    ..typed_link(2, 2)
                }],
                ..typed_span(1, "s")
            }],
        )]
    );
    assert_eq!(encode(&events), vec![rs]);
}

#[test]
fn promotes_semantic_convention_slots() {
    let rs = ResourceSpans::from_parts(
        vec![PbSpan::from_parts(1, 1, "s")],
        vec![
            KeyValue::from_string("service.name", "svc"),
            KeyValue::from_string("deployment.environment.name", "prod"),
            KeyValue::from_string("host.name", "h1"),
            KeyValue::from_string("custom", "keep"),
        ],
    );
    assert_eq!(
        rs.convert(),
        vec![resource_event(Resource {
            service: Some("svc".into()),
            environment: Some("prod".into()),
            host: Some("h1".into()),
            attributes: attrs([("custom", AttrValue::String("keep".into()))]),
            ..Resource::default()
        })]
    );
}

#[test]
fn empty_or_non_string_promotion_keys_remain() {
    let rs = ResourceSpans::from_parts(
        vec![PbSpan::from_parts(1, 1, "s")],
        vec![
            KeyValue::from_string("service.name", ""),
            KeyValue::from_int("host.name", 7),
            KeyValue::from_string("deployment.environment", "legacy-env"),
        ],
    );
    assert_eq!(
        rs.convert(),
        vec![resource_event(Resource {
            environment: Some("legacy-env".into()),
            attributes: attrs([
                ("host.name", AttrValue::Integer(7)),
                ("service.name", AttrValue::String(String::new())),
            ]),
            ..Resource::default()
        })]
    );
}

#[test]
fn stable_environment_wins_over_deprecated() {
    let rs = ResourceSpans::from_parts(
        vec![PbSpan::from_parts(1, 1, "s")],
        vec![
            KeyValue::from_string("deployment.environment", "old"),
            KeyValue::from_string("deployment.environment.name", "new"),
        ],
    );
    assert_eq!(
        rs.decode(),
        (
            vec![resource_event(Resource {
                environment: Some("new".into()),
                dropped_attributes_count: DroppedCount::new(1),
                ..Resource::default()
            })],
            TraceConversionCounts {
                attribute_collisions: 1,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn duplicate_keys_last_wins_and_increment_dropped() {
    let rs = ResourceSpans::from_parts(
        vec![PbSpan::from_parts(1, 1, "s")],
        vec![
            KeyValue::from_string("k", "a"),
            KeyValue::from_string("k", "b"),
        ],
    );
    assert_eq!(
        rs.decode(),
        (
            vec![resource_event(Resource {
                attributes: attrs([("k", AttrValue::String("b".into()))]),
                dropped_attributes_count: DroppedCount::new(1),
                ..Resource::default()
            })],
            TraceConversionCounts {
                duplicate_attributes: 1,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn reversed_timestamps_clamp_duration_to_zero() {
    let span = PbSpan {
        start_time_unix_nano: 5_000,
        end_time_unix_nano: 1_000,
        ..PbSpan::from_parts(1, 1, "s")
    };
    let (events, counts) = ResourceSpans::from_spans(vec![span.clone()]).decode();
    assert_eq!(
        events,
        vec![typed_event(
            1,
            vec![Span {
                start_time: DateTime::from_timestamp_nanos(5_000),
                duration: Duration::ZERO,
                ..typed_span(1, "s")
            }],
        )]
    );
    assert_eq!(
        counts,
        TraceConversionCounts {
            reversed_span_timestamps: 1,
            ..TraceConversionCounts::default()
        }
    );
    assert_eq!(
        encode(&events),
        vec![ResourceSpans::from_spans(vec![PbSpan {
            end_time_unix_nano: 5_000,
            ..span
        }])]
    );
}

#[test]
fn unset_and_ok_status_drop_message() {
    let ok = PbSpan {
        status: Some(PbStatus {
            code: SpanStatus::OK,
            message: "nope".into(),
        }),
        ..PbSpan::from_parts(1, 1, "ok")
    };
    let error = PbSpan {
        status: Some(PbStatus {
            code: SpanStatus::ERROR,
            message: "boom".into(),
        }),
        ..PbSpan::from_parts(1, 2, "error")
    };
    assert_eq!(
        ResourceSpans::from_spans(vec![ok, error]).decode(),
        (
            vec![typed_event(
                1,
                vec![
                    Span {
                        status: SpanStatus::Ok,
                        ..typed_span(1, "ok")
                    },
                    Span {
                        status: SpanStatus::Error("boom".into()),
                        ..typed_span(2, "error")
                    },
                ],
            )],
            TraceConversionCounts {
                nonconforming_statuses: 1,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn unknown_kind_and_status_round_trip() {
    let rs = ResourceSpans::from_spans(vec![PbSpan {
        kind: 99,
        status: Some(PbStatus {
            code: 7,
            message: "future".into(),
        }),
        ..PbSpan::from_parts(1, 1, "s")
    }]);
    assert_eq!(encode(&rs.clone().convert()), vec![rs]);
}

#[test]
fn any_value_variants_round_trip() {
    let span = PbSpan {
        attributes: vec![
            KeyValue::from_string("s", "x"),
            KeyValue::from_parts("b", Some(AnyValue::from_bytes(vec![0, 255]))),
            KeyValue::from_parts("n", None),
            KeyValue::from_parts("f", Some(AnyValue::from_double(f64::NAN))),
        ],
        ..PbSpan::from_parts(1, 1, "s")
    };
    let mut event = convert_span(span);
    let nan = event.spans_mut()[0].attributes.remove("f");
    assert!(matches!(nan, Some(AttrValue::Float(v)) if v.is_nan()));
    assert_eq!(
        event,
        typed_event(
            1,
            vec![Span {
                attributes: attrs([
                    ("b", AttrValue::Bytes(Bytes::from_static(&[0, 255]))),
                    ("n", AttrValue::Null),
                    ("s", AttrValue::String("x".into())),
                ]),
                ..typed_span(1, "s")
            }],
        )
    );
}

#[test]
fn datadog_bridge_keys_lift_and_resynthesize() {
    let span = PbSpan {
        attributes: vec![
            KeyValue::from_string("datadog.span.resource", "GET /"),
            KeyValue::from_string("datadog.span.type", "web"),
            KeyValue::from_int("datadog.chunk.priority", 0),
        ],
        ..PbSpan::from_parts(1, 1, "s")
    };
    let rs = ResourceSpans::from_parts(
        vec![span],
        vec![KeyValue::from_parts(
            "datadog.agent",
            Some(AnyValue::from_kvlist(vec![KeyValue::from_string(
                "host_name",
                "agent-1",
            )])),
        )],
    );
    let events = rs.convert();

    let mut expected = typed_event(
        1,
        vec![Span {
            datadog: DatadogSpanContext {
                resource_name: Some("GET /".into()),
                span_type: Some("web".into()),
                ..DatadogSpanContext::default()
            },
            ..typed_span(1, "s")
        }],
    );
    expected.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
        host_name: "agent-1".into(),
        ..DatadogAgentEnvelope::default()
    }));
    expected.datadog_mut().set_chunk(Some(DatadogChunkContext {
        priority: Some(SamplingPriority::AutoReject),
        ..DatadogChunkContext::default()
    }));
    assert_eq!(events, vec![expected]);

    let agent = AnyValue::from_kvlist(vec![
        KeyValue::from_string("agent_version", ""),
        KeyValue::from_string("env", ""),
        KeyValue::from_parts("error_tps", Some(AnyValue::from_double(0.0))),
        KeyValue::from_string("host_name", "agent-1"),
        KeyValue::from_parts("rare_sampler_enabled", Some(AnyValue::from_bool(false))),
        KeyValue::from_parts("tags", Some(AnyValue::from_kvlist(Vec::new()))),
        KeyValue::from_parts("target_tps", Some(AnyValue::from_double(0.0))),
    ]);
    assert_eq!(
        encode(&events),
        vec![ResourceSpans::from_parts(
            vec![PbSpan {
                attributes: vec![
                    KeyValue::from_int("datadog.chunk.priority", 0),
                    KeyValue::from_string("datadog.span.resource", "GET /"),
                    KeyValue::from_string("datadog.span.type", "web"),
                ],
                ..PbSpan::from_parts(1, 1, "s")
            }],
            vec![KeyValue::from_parts("datadog.agent", Some(agent))],
        )]
    );
}

#[test]
fn malformed_bridge_keys_are_stripped() {
    for attribute in [
        KeyValue::from_string("datadog.chunk.priority", "nope"),
        KeyValue::from_int("datadog.chunk.priority", i64::from(i32::MAX) + 1),
        KeyValue::from_string("datadog.tracer.tags", "resource-scope key on a span"),
    ] {
        let span = PbSpan {
            attributes: vec![attribute.clone()],
            ..PbSpan::from_parts(1, 1, "s")
        };
        assert_eq!(
            ResourceSpans::from_spans(vec![span]).decode(),
            (
                vec![typed_event(
                    1,
                    vec![Span {
                        dropped_attributes_count: DroppedCount::new(1),
                        ..typed_span(1, "s")
                    }],
                )],
                TraceConversionCounts {
                    invalid_reserved_attributes: 1,
                    ..TraceConversionCounts::default()
                },
            ),
            "{attribute:?}"
        );
    }
}

#[test]
fn unrecognized_datadog_keys_are_ordinary_attributes() {
    let rs = ResourceSpans::from_parts(
        vec![PbSpan {
            attributes: vec![KeyValue::from_string("datadog.custom", "x")],
            ..PbSpan::from_parts(1, 1, "s")
        }],
        vec![KeyValue::from_string("datadog.host.name", "h")],
    );
    let mut expected = resource_event(Resource {
        attributes: attrs([("datadog.host.name", AttrValue::String("h".into()))]),
        ..Resource::default()
    });
    expected.spans_mut()[0].attributes = attrs([("datadog.custom", AttrValue::String("x".into()))]);

    let events = rs.clone().convert();
    assert_eq!(events, vec![expected]);
    assert_eq!(encode(&events), vec![rs]);
}

#[test]
fn transform_authored_reserved_keys_are_stripped_on_egress() {
    let mut span = typed_span(1, "s");
    span.attributes
        .insert("datadog.span.type", AttrValue::String("web".into()));
    let (encoded, counts) = encode_counted(&[typed_event(1, vec![span])]);
    assert_eq!(
        encoded,
        vec![ResourceSpans::from_spans(vec![PbSpan {
            dropped_attributes_count: 1,
            ..PbSpan::from_parts(1, 1, "s")
        }])]
    );
    assert_eq!(
        counts,
        TraceConversionCounts {
            invalid_reserved_attributes: 1,
            ..TraceConversionCounts::default()
        }
    );
}

#[test]
fn empty_events_are_not_coalesced_and_are_reported() {
    let out = encode_counted(&[TraceEvent::new(tid(1)), TraceEvent::new(tid(2))]);
    assert_eq!(
        out,
        (
            vec![ResourceSpans {
                resource: Some(PbResource::default()),
                scope_spans: vec![
                    ScopeSpans::from_spans(Vec::new()),
                    ScopeSpans::from_spans(Vec::new()),
                ],
                schema_url: String::new(),
            }],
            TraceConversionCounts {
                empty_events: 2,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn coalesces_compatible_partitions_and_keeps_one_scope_dropped_count() {
    let mut rs = ResourceSpans::from_spans(vec![
        PbSpan::from_parts(1, 1, "a"),
        PbSpan::from_parts(2, 2, "b"),
    ]);
    rs.scope_spans[0].scope = Some(InstrumentationScope {
        name: "lib".into(),
        dropped_attributes_count: 4,
        ..InstrumentationScope::default()
    });
    let events = rs.clone().convert();

    let scope = Scope {
        name: Some("lib".into()),
        dropped_attributes_count: DroppedCount::new(4),
        ..Scope::default()
    };
    let expected: Vec<_> = [
        typed_event(1, vec![typed_span(1, "a")]),
        typed_event(2, vec![typed_span(2, "b")]),
    ]
    .into_iter()
    .map(|mut event| {
        *event.scope_mut() = scope.clone();
        event
    })
    .collect();
    assert_eq!(events, expected);
    assert_eq!(encode(&events), vec![rs]);
}

#[test]
fn incompatible_chunk_contexts_do_not_share_scope_spans() {
    let mut keep = typed_event(1, vec![typed_span(1, "a")]);
    keep.datadog_mut().set_chunk(Some(DatadogChunkContext {
        priority: Some(SamplingPriority::AutoKeep),
        ..DatadogChunkContext::default()
    }));
    let mut user = typed_event(2, vec![typed_span(2, "b")]);
    user.datadog_mut().set_chunk(Some(DatadogChunkContext {
        priority: Some(SamplingPriority::UserKeep),
        ..DatadogChunkContext::default()
    }));
    let encoded = encode(&[keep, user]);
    assert_eq!(encoded.len(), 1, "chunk state does not split ResourceSpans");
    let scopes = &encoded[0].scope_spans;
    assert_eq!(
        scopes.len(),
        2,
        "conflicting chunks stay in separate ScopeSpans"
    );
    assert_eq!(
        scopes[0].spans,
        vec![PbSpan {
            attributes: vec![KeyValue::from_int("datadog.chunk.priority", 1)],
            ..PbSpan::from_parts(1, 1, "a")
        }]
    );
    assert_eq!(
        scopes[1].spans,
        vec![PbSpan {
            attributes: vec![KeyValue::from_int("datadog.chunk.priority", 2)],
            ..PbSpan::from_parts(2, 2, "b")
        }]
    );
}

#[test]
fn incompatible_tracer_tags_do_not_share_resource_spans() {
    let mut one = typed_event(1, vec![typed_span(1, "a")]);
    one.datadog_mut().tracer.tags = attrs([("env", AttrValue::String("one".into()))]);
    let mut two = typed_event(2, vec![typed_span(2, "b")]);
    two.datadog_mut().tracer.tags = attrs([("env", AttrValue::String("two".into()))]);
    assert_eq!(
        encode(&[one.clone(), two.clone()]),
        [encode(&[one]), encode(&[two])].concat()
    );
}

#[test]
fn incompatible_agent_envelopes_do_not_share_resource_spans() {
    let mut a = typed_event(1, vec![Span::new(sid(1), "a")]);
    a.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
        host_name: "one".into(),
        ..DatadogAgentEnvelope::default()
    }));
    let mut b = typed_event(2, vec![Span::new(sid(2), "b")]);
    b.datadog_mut().set_agent(Some(DatadogAgentEnvelope {
        host_name: "two".into(),
        ..DatadogAgentEnvelope::default()
    }));
    assert_eq!(
        encode(&[a.clone(), b.clone()]),
        [encode(&[a]), encode(&[b])].concat()
    );
}

#[test]
fn metadata_is_cloned_onto_every_partition() {
    let mut metadata = EventMetadata::default();
    metadata.set_source_type("opentelemetry");
    let events = ResourceSpans::from_spans(vec![
        PbSpan::from_parts(1, 1, "a"),
        PbSpan::from_parts(2, 2, "b"),
    ])
    .into_typed_events(&metadata, &mut PanicOnIssue);
    assert_eq!(
        events,
        vec![
            typed_event_with_metadata(1, vec![typed_span(1, "a")], &metadata),
            typed_event_with_metadata(2, vec![typed_span(2, "b")], &metadata),
        ]
    );
}

#[test]
fn typed_slot_wins_on_egress_and_increments_dropped() {
    let mut events = ResourceSpans::from_parts(
        vec![PbSpan::from_parts(1, 1, "s")],
        vec![KeyValue::from_string("service.name", "svc")],
    )
    .convert();
    events[0]
        .resource_mut()
        .attributes
        .insert("service.name", AttrValue::String("dup".into()));
    assert_eq!(
        encode_counted(&events),
        (
            vec![ResourceSpans {
                resource: Some(PbResource {
                    attributes: vec![KeyValue::from_string("service.name", "svc")],
                    dropped_attributes_count: 1,
                }),
                ..ResourceSpans::from_spans(vec![PbSpan::from_parts(1, 1, "s")])
            }],
            TraceConversionCounts {
                attribute_collisions: 1,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn nested_kvlist_duplicate_keys_increment_enclosing_dropped() {
    let span = PbSpan {
        attributes: vec![KeyValue::from_parts(
            "nested",
            Some(AnyValue::from_kvlist(vec![
                KeyValue::from_string("k", "a"),
                KeyValue::from_string("k", "b"),
            ])),
        )],
        ..PbSpan::from_parts(1, 1, "s")
    };
    assert_eq!(
        ResourceSpans::from_spans(vec![span]).decode(),
        (
            vec![typed_event(
                1,
                vec![Span {
                    attributes: attrs([(
                        "nested",
                        AttrValue::Map(attr_map([("k", AttrValue::String("b".into()))])),
                    )]),
                    dropped_attributes_count: DroppedCount::new(1),
                    ..typed_span(1, "s")
                }],
            )],
            TraceConversionCounts {
                duplicate_attributes: 1,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn encoding_keeps_finalizers_until_they_are_dropped() {
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let event = typed_event(1, vec![typed_span(1, "s")]).with_batch_notifier(&batch);
    // Partitions share one metadata `Arc`, so the last clone is what acknowledges the batch.
    let shared = event.clone();
    let (_request, finalizers) =
        ExportTraceServiceRequest::from_events([event, shared], &mut PanicOnIssue);
    drop(batch);
    assert!(
        receiver.try_recv().is_err(),
        "encoding must not acknowledge the batch"
    );
    drop(finalizers);
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
}

#[test]
fn export_request_wraps_resource_spans() {
    let rs = ResourceSpans::from_spans(vec![PbSpan::from_parts(1, 1, "s")]);
    let events = rs.clone().convert();
    assert_eq!(
        ExportTraceServiceRequest::from_events(events, &mut PanicOnIssue).0,
        ExportTraceServiceRequest {
            resource_spans: vec![rs],
        }
    );
}

#[test]
fn chunk_conflict_keeps_first_value() {
    let a = PbSpan {
        attributes: vec![KeyValue::from_int("datadog.chunk.priority", 1)],
        ..PbSpan::from_parts(1, 1, "a")
    };
    let b = PbSpan {
        attributes: vec![KeyValue::from_int("datadog.chunk.priority", 2)],
        ..PbSpan::from_parts(1, 2, "b")
    };
    let mut expected = typed_event(1, vec![typed_span(1, "a"), typed_span(2, "b")]);
    expected.datadog_mut().set_chunk(Some(DatadogChunkContext {
        priority: Some(SamplingPriority::AutoKeep),
        ..DatadogChunkContext::default()
    }));
    assert_eq!(
        ResourceSpans::from_spans(vec![a, b]).decode(),
        (
            vec![expected],
            TraceConversionCounts {
                conflicting_chunk_contexts: 1,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn default_chunk_some_without_priority_does_not_survive_otlp() {
    let mut event = typed_event(1, vec![Span::new(sid(1), "s")]);
    let expected = event.clone();
    event
        .datadog_mut()
        .set_chunk(Some(DatadogChunkContext::default()));
    let (encoded, counts) = encode_counted(std::slice::from_ref(&event));
    assert_eq!(
        counts,
        TraceConversionCounts {
            unrepresentable_chunk_contexts: 1,
            ..TraceConversionCounts::default()
        }
    );
    let back = encoded.into_iter().next().unwrap().convert();
    assert_eq!(back, vec![expected]);
}

#[test]
fn keyless_chunk_coalesces_with_absent_chunk() {
    let plain = typed_event(1, vec![typed_span(1, "a")]);
    let mut keyless = typed_event(2, vec![typed_span(2, "b")]);
    keyless
        .datadog_mut()
        .set_chunk(Some(DatadogChunkContext::default()));
    assert_eq!(
        encode_counted(&[plain, keyless]),
        (
            vec![ResourceSpans::from_spans(vec![
                PbSpan::from_parts(1, 1, "a"),
                PbSpan::from_parts(2, 2, "b"),
            ])],
            TraceConversionCounts {
                unrepresentable_chunk_contexts: 1,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn timestamps_beyond_i64_nanos_round_trip() {
    let rs = ResourceSpans::from_spans(vec![PbSpan {
        start_time_unix_nano: u64::MAX - 1,
        end_time_unix_nano: u64::MAX,
        events: vec![PbEvent {
            time_unix_nano: u64::MAX,
            ..PbEvent::default()
        }],
        ..PbSpan::from_parts(1, 1, "s")
    }]);
    assert_eq!(encode(&rs.clone().convert()), vec![rs]);
}

#[test]
fn egress_clamps_timestamps_to_otlp_domain() {
    let span = Span {
        start_time: DateTime::from_timestamp(-1, 0).unwrap(),
        duration: Duration::from_secs(u64::MAX),
        events: vec![SpanEvent {
            name: String::new(),
            time: DateTime::from_timestamp(i64::from(u32::MAX) * 10, 0).unwrap(),
            attributes: Attributes::new(),
            dropped_attributes_count: DroppedCount::ZERO,
        }],
        ..typed_span(1, "s")
    };
    let (encoded, counts) = encode_counted(&[typed_event(1, vec![span])]);
    let wire = &encoded[0].scope_spans[0].spans[0];
    assert_eq!(
        (
            wire.start_time_unix_nano,
            wire.end_time_unix_nano,
            wire.events[0].time_unix_nano,
            counts,
        ),
        (
            0,
            u64::MAX,
            u64::MAX,
            TraceConversionCounts {
                clamped_timestamps: 3,
                ..TraceConversionCounts::default()
            },
        )
    );
}

/// Generated keys contain no `.`, so they never match a promoted or reserved `datadog.*` key.
const KEY: &str = "[a-z]{1,6}";

/// Every OTLP `AnyValue` shape except NaN doubles, which prost compares as unequal to
/// themselves.
fn any_value_strategy() -> impl Strategy<Value = AnyValue> {
    let leaf = prop_oneof![
        Just(AnyValue { value: None }),
        "[ -~]{0,8}".prop_map(AnyValue::from_string),
        prop::collection::vec(any::<u8>(), 0..8).prop_map(AnyValue::from_bytes),
        any::<bool>().prop_map(AnyValue::from_bool),
        any::<i64>().prop_map(AnyValue::from_int),
        any::<f64>()
            .prop_filter("finite", |v| v.is_finite())
            .prop_map(AnyValue::from_double),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(|values| AnyValue {
                value: Some(PbValue::ArrayValue(ArrayValue { values })),
            }),
            unique_key_values(inner).prop_map(AnyValue::from_kvlist),
        ]
    })
}

/// Unique keys in sorted order, matching what egress emits from the typed `BTreeMap`.
fn unique_key_values(
    value: impl Strategy<Value = AnyValue>,
) -> impl Strategy<Value = Vec<KeyValue>> {
    prop::collection::btree_map(KEY, value, 0..4).prop_map(|map| {
        map.into_iter()
            .map(|(key, value)| KeyValue::from_parts(key, Some(value)))
            .collect()
    })
}

fn attributes_strategy() -> impl Strategy<Value = Vec<KeyValue>> {
    unique_key_values(any_value_strategy())
}

fn event_strategy() -> impl Strategy<Value = PbEvent> {
    (
        any::<u64>(),
        "[a-z]{0,6}",
        attributes_strategy(),
        any::<u32>(),
    )
        .prop_map(
            |(time_unix_nano, name, attributes, dropped_attributes_count)| PbEvent {
                time_unix_nano,
                name,
                attributes,
                dropped_attributes_count,
            },
        )
}

fn link_strategy() -> impl Strategy<Value = PbLink> {
    (
        1..=u128::MAX,
        1..=u64::MAX,
        "[a-z=,]{0,8}",
        any::<u32>(),
        attributes_strategy(),
        any::<u32>(),
    )
        .prop_map(
            |(trace, span, trace_state, flags, attributes, dropped_attributes_count)| PbLink {
                trace_state,
                flags,
                attributes,
                dropped_attributes_count,
                ..PbLink::from_parts(trace, span)
            },
        )
}

/// Statuses that survive ingest unchanged: `UNSET` and `OK` carry no message.
fn status_strategy() -> impl Strategy<Value = PbStatus> {
    prop_oneof![
        (SpanStatus::UNSET..=SpanStatus::OK).prop_map(|code| PbStatus {
            code,
            message: String::new(),
        }),
        (
            prop_oneof![i32::MIN..SpanStatus::UNSET, SpanStatus::ERROR..=i32::MAX],
            "[a-z ]{0,8}"
        )
            .prop_map(|(code, message)| PbStatus { code, message }),
    ]
}

/// Spans drawn from a small trace-ID pool so groupings interleave several trace IDs.
fn span_strategy() -> impl Strategy<Value = PbSpan> {
    (
        (
            1..=3_u128,
            1..=u64::MAX,
            prop::option::of(1..=u64::MAX),
            "[a-z=,]{0,8}",
            any::<u32>(),
        ),
        (
            "[a-z]{0,8}",
            any::<i32>(),
            any::<u64>(),
            any::<u64>(),
            status_strategy(),
        ),
        (
            attributes_strategy(),
            prop::collection::vec(event_strategy(), 0..3),
            prop::collection::vec(link_strategy(), 0..3),
            any::<(u32, u32, u32)>(),
        ),
    )
        .prop_map(
            |(
                (trace, span, parent, trace_state, flags),
                (name, kind, start, duration, status),
                (attributes, events, links, (dropped_attributes, dropped_events, dropped_links)),
            )| PbSpan {
                parent_span_id: parent
                    .map(|parent| sid(parent).to_bytes().to_vec())
                    .unwrap_or_default(),
                trace_state,
                flags,
                kind,
                start_time_unix_nano: start,
                end_time_unix_nano: start.saturating_add(duration),
                attributes,
                dropped_attributes_count: dropped_attributes,
                events,
                dropped_events_count: dropped_events,
                links,
                dropped_links_count: dropped_links,
                status: Some(status),
                ..PbSpan::from_parts(trace, span, &name)
            },
        )
}

fn resource_spans_strategy() -> impl Strategy<Value = ResourceSpans> {
    (
        (attributes_strategy(), any::<u32>(), "[a-z]{0,4}"),
        (
            "[a-z]{0,4}",
            "[a-z]{0,4}",
            attributes_strategy(),
            any::<u32>(),
            "[a-z]{0,4}",
        ),
        prop::collection::vec(span_strategy(), 1..6),
    )
        .prop_map(
            |(
                (attributes, dropped_attributes_count, schema_url),
                (name, version, scope_attributes, scope_dropped, scope_schema_url),
                spans,
            )| ResourceSpans {
                resource: Some(PbResource {
                    attributes,
                    dropped_attributes_count,
                }),
                scope_spans: vec![ScopeSpans {
                    scope: Some(InstrumentationScope {
                        name,
                        version,
                        attributes: scope_attributes,
                        dropped_attributes_count: scope_dropped,
                    }),
                    spans,
                    schema_url: scope_schema_url,
                }],
                schema_url,
            },
        )
}

/// The span order egress produces: trace-ID partitions in first-seen order, each keeping
/// its spans' wire order.
fn group_by_first_seen_trace_id(spans: Vec<PbSpan>) -> Vec<PbSpan> {
    let mut groups: Vec<(Vec<u8>, Vec<PbSpan>)> = Vec::new();
    for span in spans {
        match groups.iter_mut().find(|(id, _)| *id == span.trace_id) {
            Some((_, group)) => group.push(span),
            None => groups.push((span.trace_id.clone(), vec![span])),
        }
    }
    groups.into_iter().flat_map(|(_, group)| group).collect()
}

const DOTTED_KEY: &str = "[a-z]{1,3}(\\.[a-z]{1,3}){0,2}";

const PROMOTED_KEYS: &[&str] = &[
    SERVICE_NAME,
    ENVIRONMENT_NAME,
    ENVIRONMENT_LEGACY,
    HOST_NAME,
];

const RESERVED_KEYS: &[&str] = &[
    DATADOG_AGENT,
    DATADOG_TRACER_TAGS,
    DATADOG_CHUNK_PRIORITY,
    DATADOG_CHUNK_ORIGIN,
    DATADOG_CHUNK_DROPPED,
    DATADOG_CHUNK_TAGS,
    DATADOG_SPAN_RESOURCE,
    DATADOG_SPAN_TYPE,
    DATADOG_SPAN_META_STRUCT,
];

/// Keys that may contain dots, including the promoted semantic-convention keys, the reserved
/// bridge keys, and other `datadog.*` keys.
fn dotted_key() -> impl Strategy<Value = String> {
    prop_oneof![
        DOTTED_KEY.prop_map(String::from),
        "datadog\\.[a-z]{1,6}".prop_map(String::from),
        prop::sample::select(PROMOTED_KEYS).prop_map(String::from),
        prop::sample::select(RESERVED_KEYS).prop_map(String::from),
    ]
}

/// Possibly duplicate dotted keys with arbitrary values.
fn dotted_key_values(
    value: impl Strategy<Value = AnyValue>,
) -> impl Strategy<Value = Vec<KeyValue>> {
    prop::collection::vec((DOTTED_KEY, value), 0..4).prop_map(|entries| {
        entries
            .into_iter()
            .map(|(key, value)| KeyValue::from_parts(key, Some(value)))
            .collect()
    })
}

fn agent_member() -> impl Strategy<Value = KeyValue> {
    prop_oneof![
        (
            prop::sample::select(&["host_name", "env", "agent_version"][..]),
            "[a-z]{0,4}"
        )
            .prop_map(|(key, value)| KeyValue::from_string(key, &value)),
        (
            prop::sample::select(&["target_tps", "error_tps"][..]),
            -1e6..1e6_f64
        )
            .prop_map(|(key, value)| KeyValue::from_parts(key, Some(AnyValue::from_double(value)))),
        any::<bool>().prop_map(|value| {
            KeyValue::from_parts("rare_sampler_enabled", Some(AnyValue::from_bool(value)))
        }),
        dotted_key_values(any_value_strategy())
            .prop_map(|tags| KeyValue::from_parts("tags", Some(AnyValue::from_kvlist(tags)))),
        // A recognized member of the wrong type, or an unknown member.
        ("[a-z_]{1,12}", any_value_strategy())
            .prop_map(|(key, value)| KeyValue::from_parts(key, Some(value))),
    ]
}

/// A value for `key`: mostly the shape a promoted or reserved key lifts from, sometimes any
/// other value so malformed bridge keys are exercised too.
fn keyed_value(key: &str) -> BoxedStrategy<AnyValue> {
    let typed = match key {
        SERVICE_NAME
        | ENVIRONMENT_NAME
        | ENVIRONMENT_LEGACY
        | HOST_NAME
        | DATADOG_CHUNK_ORIGIN
        | DATADOG_SPAN_RESOURCE
        | DATADOG_SPAN_TYPE => "[a-z]{0,4}".prop_map(AnyValue::from_string).boxed(),
        DATADOG_CHUNK_PRIORITY => prop_oneof![-3_i64..4, any::<i64>()]
            .prop_map(AnyValue::from_int)
            .boxed(),
        DATADOG_CHUNK_DROPPED => any::<bool>().prop_map(AnyValue::from_bool).boxed(),
        DATADOG_CHUNK_TAGS | DATADOG_TRACER_TAGS => dotted_key_values(any_value_strategy())
            .prop_map(AnyValue::from_kvlist)
            .boxed(),
        DATADOG_SPAN_META_STRUCT => dotted_key_values(prop_oneof![
            4 => prop::collection::vec(any::<u8>(), 0..4).prop_map(AnyValue::from_bytes),
            1 => any_value_strategy(),
        ])
        .prop_map(AnyValue::from_kvlist)
        .boxed(),
        DATADOG_AGENT => prop::collection::vec(agent_member(), 0..5)
            .prop_map(AnyValue::from_kvlist)
            .boxed(),
        _ => {
            return prop_oneof![
                any_value_strategy(),
                dotted_key_values(any_value_strategy()).prop_map(AnyValue::from_kvlist),
            ]
            .boxed();
        }
    };
    prop_oneof![4 => typed, 1 => any_value_strategy()].boxed()
}

/// Possibly duplicate attributes keyed by [`dotted_key`].
fn dotted_attributes() -> impl Strategy<Value = Vec<KeyValue>> {
    prop::collection::vec(
        dotted_key().prop_flat_map(|key| {
            let value = keyed_value(&key);
            (Just(key), value)
        }),
        0..6,
    )
    .prop_map(|entries| {
        entries
            .into_iter()
            .map(|(key, value)| KeyValue::from_parts(key, Some(value)))
            .collect()
    })
}

/// [`resource_spans_strategy`] with dotted, possibly duplicate resource, scope, and span
/// attributes.
fn dotted_resource_spans_strategy() -> impl Strategy<Value = ResourceSpans> {
    (
        resource_spans_strategy(),
        dotted_attributes(),
        dotted_attributes(),
        prop::collection::vec(dotted_attributes(), 6),
    )
        .prop_map(
            |(mut rs, resource_attributes, scope_attributes, span_attributes)| {
                if let Some(resource) = &mut rs.resource {
                    resource.attributes = resource_attributes;
                }
                let scope_spans = &mut rs.scope_spans[0];
                if let Some(scope) = &mut scope_spans.scope {
                    scope.attributes = scope_attributes;
                }
                for (span, attributes) in scope_spans.spans.iter_mut().zip(span_attributes) {
                    span.attributes = attributes;
                }
                rs
            },
        )
}

fn is_promotable(value: Option<&AttrValue>) -> bool {
    matches!(value, Some(AttrValue::String(s)) if !s.is_empty())
}

proptest! {
    #[test]
    fn dotted_keys_are_lifted_promoted_and_reach_a_fixpoint(
        rs in dotted_resource_spans_strategy(),
    ) {
        // The generated input may be malformed, so the first pass may report issues.
        let mut counts = TraceConversionCounts::default();
        let first = rs.into_typed_events(&EventMetadata::default(), &mut counts);
        for event in &first {
            let attributes = &event.resource().attributes;
            prop_assert!(attributes.iter().all(|(key, _)| !is_reserved_datadog_key(key)));
            for key in PROMOTED_KEYS {
                prop_assert!(!is_promotable(attributes.get(key)), "{key} was not promoted");
            }
            prop_assert!(
                attributes.get(ENVIRONMENT_NAME).is_none()
                    || attributes.get(ENVIRONMENT_LEGACY).is_none()
            );
            for span in event.spans() {
                prop_assert!(span.attributes.iter().all(|(key, _)| !is_reserved_datadog_key(key)));
            }
        }

        // One wire round trip normalizes the input; after that, nothing is reported and the
        // typed model is stable.
        let normalized: Vec<TraceEvent> =
            ExportTraceServiceRequest::from_events(first, &mut counts).0
                .resource_spans
                .into_iter()
                .flat_map(ResourceSpans::convert)
                .collect();
        let again: Vec<TraceEvent> = encode(&normalized)
            .into_iter()
            .flat_map(ResourceSpans::convert)
            .collect();
        prop_assert_eq!(again, normalized);
    }

    #[test]
    fn typed_model_is_a_round_trip_fixpoint(rs in resource_spans_strategy()) {
        let events = rs.convert();
        let again: Vec<TraceEvent> = encode(&events)
            .into_iter()
            .flat_map(ResourceSpans::convert)
            .collect();
        prop_assert_eq!(again, events);
    }

    #[test]
    fn wire_round_trip_groups_spans_by_first_seen_trace_id(mut rs in resource_spans_strategy()) {
        let encoded = encode(&rs.clone().convert());
        let spans = std::mem::take(&mut rs.scope_spans[0].spans);
        rs.scope_spans[0].spans = group_by_first_seen_trace_id(spans);
        prop_assert_eq!(encoded, vec![rs]);
    }
}
