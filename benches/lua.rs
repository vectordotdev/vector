use criterion::{criterion_group, Benchmark, Criterion};
use indexmap::IndexMap;
use vector::{
    transforms::{self, Transform},
    Record,
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
                    transforms::lua::Lua::new(source)
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

criterion_group!(lua, add_fields);
