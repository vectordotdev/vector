use std::collections::BTreeMap;

use compiler::state;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use indoc::indoc;
use vector_common::TimeZone;
use vrl::{Runtime, Value};

static SOURCE: [(&str, &str); 2] = [
    (
        "parse_json",
        indoc! {r#"
	x = parse_json!(s'{"noog": "nork"}')
        x.noog
    "#},
    ),
    (
        "simple",
        indoc! {r#"
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
    ),
];

fn benchmark_kind_display(c: &mut Criterion) {
    let mut group = c.benchmark_group("vrl_compiler/value::kind::display");
    for (name, source) in &SOURCE {
        let state = state::Runtime::default();
        let runtime = Runtime::new(state);
        let tz = TimeZone::default();
        let functions = vrl_stdlib::all();
        let program = vrl::compile(source, &functions, None).unwrap();
        let vm = runtime.compile(functions, &program).unwrap();

        group.bench_with_input(BenchmarkId::new("Vm", name), &vm, |b, vm| {
            b.iter(|| {
                let state = state::Runtime::default();
                let mut runtime = Runtime::new(state);
                runtime.run_vm(vm, &mut Value::Object(BTreeMap::default()), &tz)
            })
        });

        group.bench_with_input(BenchmarkId::new("Ast", name), &(), |b, _| {
            b.iter(|| {
                let state = state::Runtime::default();
                let mut runtime = Runtime::new(state);
                runtime.resolve(&mut Value::Object(BTreeMap::default()), &program, &tz)
            })
        });
    }
}

criterion_group!(name = vrl_compiler_kind;
                 config = Criterion::default();
                 targets = benchmark_kind_display);
criterion_main!(vrl_compiler_kind);
