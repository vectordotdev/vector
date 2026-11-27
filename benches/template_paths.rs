use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use vector::{
    event::{Event, LogEvent, Metric, MetricKind, MetricValue},
    template::{UnconfinedTemplate, UnsignedIntTemplate},
};
use vector_lib::{lookup::metadata_path, metric_tags};
use vrl::event_path;

fn template_paths(c: &mut Criterion) {
    let mut log = LogEvent::from("hello world");
    log.insert(event_path!("service"), "frontend");
    log.insert(event_path!("host"), "edge-01");
    log.insert(event_path!("status"), 200);
    log.insert(
        event_path!("kubernetes", "labels", "app.kubernetes.io/name"),
        "frontend",
    );
    log.insert(event_path!("requests", 0, "status"), 200);
    log.insert(metadata_path!("tenant"), "customer-01");
    let trace = Event::Trace(log.clone().into());
    let event = Event::Log(log);
    let mut group = c.benchmark_group("template_paths");

    // Construct templates and events outside the measured loop: components
    // reuse their configured templates across events. Include output allocation
    // and destruction, but exclude unrelated event cloning and ingestion work.
    for (name, source) in [
        ("static_control", "frontend"),
        ("simple_field", "{{ service }}"),
        (
            "nested_quoted_field",
            r#"{{ kubernetes.labels."app.kubernetes.io/name" }}"#,
        ),
        ("array_field", "{{ requests[0].status }}"),
        ("metadata_field", "{{ %tenant }}"),
        (
            "three_fields",
            "service={{ service }},host={{ host }},status={{ status }}",
        ),
    ] {
        let template = UnconfinedTemplate::try_from(source).unwrap();
        assert!(template.render_string(&event).is_ok());
        group.bench_function(name, |b| {
            b.iter(|| black_box(template.render_string(black_box(&event)).unwrap()));
        });
    }

    let template = UnconfinedTemplate::try_from("{{ service }}").unwrap();
    assert!(template.render_string(&trace).is_ok());
    group.bench_function("trace_field", |b| {
        b.iter(|| black_box(template.render_string(black_box(&trace)).unwrap()));
    });

    let template = UnsignedIntTemplate::try_from("{{ requests[0].status }}").unwrap();
    assert_eq!(template.render(&event).unwrap(), 200);
    group.bench_function("unsigned_array_field", |b| {
        b.iter(|| black_box(template.render(black_box(&event)).unwrap()));
    });

    let metric = Metric::new(
        "requests_total",
        MetricKind::Absolute,
        MetricValue::Counter { value: 1.0 },
    )
    .with_tags(Some(metric_tags!("service" => "frontend")));
    let event = Event::Metric(metric);
    let template = UnconfinedTemplate::try_from("{{ tags.service }}").unwrap();
    assert!(template.render_string(&event).is_ok());
    group.bench_function("metric_tag_control", |b| {
        b.iter(|| black_box(template.render_string(black_box(&event)).unwrap()));
    });

    group.finish();
}

criterion_group!(benches, template_paths);
criterion_main!(benches);
