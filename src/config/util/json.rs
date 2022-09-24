use crate::config::ConfigBuilder;
use serde_json::Value;

/// Helper to merge JSON. Handles objects and array concatenation.
pub(crate) fn merge(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Object(ref mut a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k).or_insert(Value::Null), v);
            }
        }
        (a, b) => {
            *a = b;
        }
    }
}

/// Helper to sort array values.
fn sort_array_values(json: &mut Value) {
    match json {
        Value::Array(ref mut arr) => {
            for v in arr.iter_mut() {
                sort_array_values(v);
            }

            // Since `Value` does not have a native ordering, we first convert
            // to string, sort, and then convert back to `Value`.
            //
            // Practically speaking, there should not be config options that mix
            // many JSON types in a single array. This is mainly to sort fields
            // like component inputs.
            let mut a = arr
                .iter()
                .map(|v| serde_json::to_string(v).unwrap())
                .collect::<Vec<_>>();
            a.sort();
            *arr = a
                .iter()
                .map(|v| serde_json::from_str(v.as_str()).unwrap())
                .collect::<Vec<_>>();
        }
        Value::Object(ref mut json) => {
            for (_, v) in json {
                sort_array_values(v);
            }
        }
        _ => {}
    }
}

/// Convert a raw user config to a JSON string
pub(crate) fn serialize(
    source: toml::value::Table,
    source_builder: &ConfigBuilder,
    include_defaults: bool,
    pretty_print: bool,
) -> serde_json::Result<String> {
    // Convert table to JSON
    let mut source_json = serde_json::to_value(source)
        .expect("should serialize config source to JSON. Please report.");

    // If a user has requested default fields, we'll serialize a `ConfigBuilder`. Otherwise,
    // we'll serialize the raw user provided config (without interpolated env vars, to preserve
    // the original source).
    if include_defaults {
        // For security, we don't want environment variables to be interpolated in the final
        // output, but we *do* want defaults. To work around this, we'll serialize `ConfigBuilder`
        // to JSON, and merge in the raw config which will contain the pre-interpolated strings.
        let mut builder = serde_json::to_value(&source_builder)
            .expect("should serialize ConfigBuilder to JSON. Please report.");

        merge(&mut builder, source_json);

        source_json = builder
    }

    sort_array_values(&mut source_json);

    // Get a JSON string. This will either be pretty printed or (default) minified.
    if pretty_print {
        serde_json::to_string_pretty(&source_json)
    } else {
        serde_json::to_string(&source_json)
    }
}
