use std::time::Duration;

use compiler::state;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use indoc::indoc;
use vector_common::TimeZone;
use vrl::{Runtime, Value};

struct Source {
    name: &'static str,
    target: &'static str,
    program: &'static str,
}

static SOURCES: &[Source] = &[
    Source {
        name: "parse_json",
        target: r#"
            {
                "hostname": "vector",
                "timestamp": "2022-05-10T10:43:15Z"
            }"#,
        program: indoc! {r#"
            parse_json!(s'{"noog": "nork"}')
        "#},
    },
    Source {
        name: "deletions",
        target: r#"{
            "hostname": "prod-223",
            "kubernetes": {
                "container_id": "a6926c9e-a4a0-4f80-8f71-2e7dd7d59f67",
                "container_image": "gcr.io/k8s-minikube/storage-provisioner:v3",
                "namespace_labels": {
                    "kubernetes.io/metadata.name": "kube-system"
                },
                "pod_annotations": {
                    "annotation1": "sample text",
                    "annotation2": "sample text"
                },
                "pod_ip": "192.168.1.1",
                "pod_name": "storage-provisioner",
                "pod_node_name": "minikube",
                "pod_owner": "root",
                "pod_uid": "93bde4d0-9731-4785-a80e-cd27ba8ad7c2",
                "pod_labels": {
                    "addonmanager.kubernetes.io/mode": "Reconcile",
                    "gcp-auth-skip-secret": "true",
                    "integration-test": "storage-provisioner",
                    "app": "production-123"
                }
            },
            "file": "/var/log/pods/kube-system_storage-provisioner_93bde4d0-9731-4785-a80e-cd27ba8ad7c2/storage-provisioner/1.log",
            "message": "F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable",
            "source_type": "kubernetes_logs",
            "stream": "stderr",
            "timestamp": "2020-10-15T11:01:46.499555308Z"
        }"#,
        program: indoc! {r#"
            if exists(.kubernetes) {
                del(.kubernetes.container_id)
                del(.kubernetes.container_image)
                del(.kubernetes.namespace_labels)
                del(.kubernetes.pod_annotations)
                del(.kubernetes.pod_ip)
                del(.kubernetes.pod_name)
                del(.kubernetes.pod_node_name)
                del(.kubernetes.pod_owner)
                del(.kubernetes.pod_uid)
                del(.kubernetes.pod_labels.app)
            }
        "#},
    },
    Source {
        name: "simple",
        target: "{}",
        program: indoc! {r#"
            .hostname = "vector"

            if .status == "warning" {
                .thing = upcase(.hostname)
            } else if .status == "notice" {
                .thung = downcase(.hostname)
            } else {
                .nong = upcase(.hostname)
            }

            .matches = { "name": .message, "num": "2" }
            .origin, .err = .hostname + "/" + .matches.name + "/" + .matches.num
        "#},
    },
];

fn benchmark_vrl_runtimes(c: &mut Criterion) {
    let mut group = c.benchmark_group("vrl/runtime");
    for source in SOURCES {
        let state = state::Runtime::default();
        let runtime = Runtime::new(state);
        let tz = TimeZone::default();
        let functions = vrl_stdlib::all();
        let (program, _) = vrl::compile(source.program, &functions).unwrap();
        let mut external_env = state::ExternalEnv::default();
        let vm = runtime
            .compile(functions, &program, &mut external_env)
            .unwrap();

        group.bench_with_input(BenchmarkId::new(source.name, "vm"), &vm, |b, vm| {
            let state = state::Runtime::default();
            let mut runtime = Runtime::new(state);
            let target: Value = serde_json::from_str(source.target).expect("valid json");

            b.iter_with_setup(
                || target.clone(),
                |mut obj| {
                    let _ = black_box(runtime.run_vm(vm, &mut obj, &tz));
                    runtime.clear();
                    obj
                },
            )
        });

        group.bench_with_input(BenchmarkId::new(source.name, "ast"), &(), |b, _| {
            let state = state::Runtime::default();
            let mut runtime = Runtime::new(state);
            let target: Value = serde_json::from_str(source.target).expect("valid json");

            b.iter_with_setup(
                || target.clone(),
                |mut obj| {
                    let _ = black_box(runtime.resolve(&mut obj, &program, &tz));
                    runtime.clear();
                    obj
                },
            )
        });
    }
}

criterion_group!(name = vrl_runtime;
                config = Criterion::default()
                    .warm_up_time(Duration::from_secs(5))
                    .measurement_time(Duration::from_secs(30))
                    // degree of noise to ignore in measurements, here 1%
                    .noise_threshold(0.01)
                    // likelihood of noise registering as difference, here 5%
                    .significance_level(0.05)
                    // likelihood of capturing the true runtime, here 95%
                    .confidence_level(0.95)
                    // total number of bootstrap resamples, higher is less noisy but slower
                    .nresamples(100_000)
                    // total samples to collect within the set measurement time
                    .sample_size(150);
                 targets = benchmark_vrl_runtimes);
criterion_main!(vrl_runtime);
