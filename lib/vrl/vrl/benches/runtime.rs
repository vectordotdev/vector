use std::collections::BTreeMap;

use compiler::state;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use indoc::indoc;
use vector_common::TimeZone;
use vrl::{Runtime, Value};

struct Source {
    name: &'static str,
    code: &'static str,
}

static SOURCES: [Source; 2] = [
    Source {
        name: "parse_json",
        code: indoc! {r#"
            x = parse_json!(s'{"noog": "nork"}')
            x.noog
        "#},
    },
    Source {
        name: "simple",
        code: indoc! {r#"
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

fn benchmark_kind_display(c: &mut Criterion) {
    let mut group = c.benchmark_group("vrl/runtime");
    for source in &SOURCES {
        let state = state::Runtime::default();
        let runtime = Runtime::new(state);
        let tz = TimeZone::default();
        let functions = vrl_stdlib::all();
        let (program, _) = vrl::compile(source.code, &functions).unwrap();
        let mut external_env = state::ExternalEnv::default();
        let vm = runtime
            .compile(functions, &program, &mut external_env)
            .unwrap();

        group.bench_with_input(BenchmarkId::new("Vm", source.name), &vm, |b, vm| {
            let state = state::Runtime::default();
            let mut runtime = Runtime::new(state);
            b.iter_with_setup(
                || Value::Object(BTreeMap::default()),
                |mut obj| {
                    let _ = black_box(runtime.run_vm(vm, &mut obj, &tz));
                    runtime.clear();
                    obj // Return the obj so it doesn't get dropped.
                },
            )
        });

        group.bench_with_input(BenchmarkId::new("Ast", source.name), &(), |b, _| {
            let state = state::Runtime::default();
            let mut runtime = Runtime::new(state);
            b.iter_with_setup(
                || Value::Object(BTreeMap::default()),
                |mut obj| {
                    let _ = black_box(runtime.resolve(&mut obj, &program, &tz));
                    runtime.clear();
                    obj
                },
            )
        });
    }
}

criterion_group!(name = vrl_compiler_kind;
                 config = Criterion::default();
                 targets = benchmark_kind_display);
criterion_main!(vrl_compiler_kind);
