use criterion::{criterion_group, criterion_main, Criterion};
use remap::prelude::*;

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.02);
    targets = upcase, downcase, parse_json
);
criterion_main!(benches);

bench_function! {
    upcase => remap_functions::Upcase;

    literal_value {
        args: func_args![value: "foo"],
        want: Ok("FOO")
    }
}

bench_function! {
    downcase => remap_functions::Downcase;

    literal_value {
        args: func_args![value: "FOO"],
        want: Ok("foo")
    }
}

bench_function! {
    parse_json => remap_functions::ParseJson;

    literal_value {
        args: func_args![value: r#"{"key": "value"}"#],
        want: Ok(value!({"key": "value"})),
    }
}
