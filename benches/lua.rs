use criterion::{criterion_group, Benchmark, Criterion};
use indexmap::IndexMap;
use vector::{
    transforms::{self, Transform},
    Record,
    topology::config::TransformConfig,
};

fn add_fields(c: &mut Criterion) {
    let num_records: usize = 100_000;

    let key = "the key";
    let value = "this is the value";

    let key_atom = key.into();
    let value_bytes = value.into();
    let key_atom2 = key.into();
    let value_bytes2 = value.into();

    c.bench(
        "lua_add_fields",
        Benchmark::new("native", move |b| {
            b.iter_with_setup(
                || {
                    let mut map = IndexMap::new();
                    map.insert(key.into(), value.to_owned());
                    transforms::add_fields::AddFields::new(map)
                },
                |transform| {
                    for _ in 0..num_records {
                        let record = Record::new_empty();
                        let record = transform.transform(record).unwrap();
                        assert_eq!(record[&key_atom], value_bytes);
                    }
                },
            )
        })
        .with_function("lua", move |b| {
            b.iter_with_setup(
                || {
                    let source = format!("record['{}'] = '{}'", key, value);
                    transforms::lua::Lua::new(&source).unwrap()
                },
                |transform| {
                    for _ in 0..num_records {
                        let record = Record::new_empty();
                        let record = transform.transform(record).unwrap();
                        assert_eq!(record[&key_atom2], value_bytes2);
                    }
                },
            )
        })
        .sample_size(10),
    );
}

fn field_filter(c: &mut Criterion) {
    let num_records: usize = 100_000;

    c.bench(
        "lua_field_filter",
        Benchmark::new("native", move |b| {
            b.iter_with_setup(
                || {
                    transforms::field_filter::FieldFilterConfig { field: "the_field".to_string(), value: "0".to_string() }.build().unwrap()
                },
                |transform| {
                    let num = (0..num_records)
                        .map(|i| {
                            let mut record = Record::new_empty();
                            record.insert_explicit("the_field".into(), (i % 10).to_string().into());
                            record
                        })
                        .filter_map(|r| transform.transform(r))
                        .count();
                    assert_eq!(num, num_records / 10);
                },
            )
        })
        .with_function("lua", move |b| {
            b.iter_with_setup(
                || {
                    let source = r#"
                      if record["the_field"] ~= "0" then
                        record = nil
                      end
                    "#;
                    transforms::lua::Lua::new(&source).unwrap()
                },
                |transform| {
                    let num = (0..num_records)
                        .map(|i| {
                            let mut record = Record::new_empty();
                            record.insert_explicit("the_field".into(), (i % 10).to_string().into());
                            record
                        })
                        .filter_map(|r| transform.transform(r))
                        .count();
                    assert_eq!(num, num_records / 10);
                },
            )
        })
        .sample_size(10),
    );
}

criterion_group!(lua, add_fields, field_filter);
