use std::collections::BTreeMap;
use vrl::{path, path::ValuePath, value::{KeyString, ObjectMap, Value}};
include!(concat!(env!("OUT_DIR"), "/helper.rs"));

fn insert_map(root: &mut Value, name: &str, values: &BTreeMap<String, String>, bulk: bool) {
    if bulk {
        if !values.is_empty() { merge_metadata_map(root, path!("kubernetes", name), values); }
    } else {
        for (key, value) in values {
            root.insert(path!("kubernetes", name, key), Value::from(value.clone()));
        }
    }
}

fn main() {
    println!("labels,annotations,existing,baseline_comparisons,bulk_comparisons,saved,saved_pct");
    for count in [0, 1, 10, 21, 50] {
        let mut labels: BTreeMap<_, _> = (0..count).map(|i| (format!("bench.example/key{i:02}"), format!("value{i:02}"))).collect();
        if count == 21 { labels.remove("bench.example/key20"); labels.insert("benchmark".into(), "vrl-kind-ab".into()); }
        let annotations: BTreeMap<_, _> = (0..10).map(|i| (format!("bench.example/annotation{i:02}"), format!("value{i:02}"))).collect();
        for existing in [false, true] {
            let mut initial = Value::Object(ObjectMap::new());
            for field in ["message", "timestamp", "stream", "file"] { initial.insert(path!(field), Value::from("fixture")); }
            for field in ["pod_name", "pod_namespace", "pod_uid"] { initial.insert(path!("kubernetes", field), Value::from("fixture")); }
            if existing {
                let retained = BTreeMap::from([("retained".into(), "keep".into())]);
                insert_map(&mut initial, "pod_labels", &retained, false);
                insert_map(&mut initial, "pod_annotations", &retained, false);
            }
            let mut results = Vec::new();
            let mut counts = Vec::new();
            for bulk in [false, true] {
                let mut root = initial.clone();
                KeyString::take_comparison_count();
                insert_map(&mut root, "pod_labels", &labels, bulk);
                insert_map(&mut root, "pod_annotations", &annotations, bulk);
                counts.push(KeyString::take_comparison_count());
                results.push(root);
            }
            assert_eq!(results[0], results[1]);
            let saved = counts[0] as i64 - counts[1] as i64;
            println!("{count},10,{existing},{},{},{saved},{:.2}", counts[0], counts[1], 100.0 * saved as f64 / counts[0] as f64);
        }
    }
}
