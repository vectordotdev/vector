use apache_avro::{types::Value, Decimal, Schema};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

const FIXTURES_PATH: &str = "lib/codecs/tests/data/avro/generated";

fn generate_avro_test_case_boolean() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "bool_field", "type": "boolean", "default": false}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        bool_field: bool,
    }
    let value = Test { bool_field: true };
    generate_test_case(schema, value, "boolean");
}

fn generate_avro_test_case_int() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "int_field", "type": "int", "default": 0}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        int_field: i32,
    }
    let value = Test { int_field: 1234 };
    generate_test_case(schema, value, "int");
}

fn generate_avro_test_case_long() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "long_field", "type": "long", "default": 0}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        long_field: i64,
    }
    let value = Test {
        long_field: 42949672960i64,
    };
    generate_test_case(schema, value, "long");
}

fn generate_avro_test_case_float() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "float_field", "type": "float", "default": 0}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        float_field: f32,
    }
    let value = Test {
        float_field: 123.456,
    };
    generate_test_case(schema, value, "float");
}

fn generate_avro_test_case_double() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "double_field", "type": "double", "default": 0}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        double_field: f64,
    }
    let value = Test {
        double_field: 123.456f64,
    };
    generate_test_case(schema, value, "double");
}

fn generate_avro_test_case_bytes() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "bytes_field", "type": "bytes"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        bytes_field: Vec<u8>,
    }
    let value = Test {
        bytes_field: vec![1, 2, 3, 4, 5, 6, 6, 7],
    };
    generate_test_case(schema, value, "bytes");
}

fn generate_avro_test_case_string() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "string_field", "type": "string"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        string_field: String,
    }
    let value = Test {
        string_field: "hello world!".to_string(),
    };
    generate_test_case(schema, value, "string");
}

#[allow(unused)]
fn generate_avro_test_case_fixed() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "fixed_field", "type":"fixed", "size": 16}
        ]
    }
    "#;
    let record = Value::Record(vec![(
        "fixed_field".into(),
        Value::Fixed(16, b"1019181716151413".to_vec()),
    )]);
    generate_test_case_from_value(schema, record, "fixed");
}

fn generate_avro_test_case_enum() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "enum_field", "type": "enum", "symbols" : ["Spades", "Hearts", "Diamonds", "Clubs"]}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    enum Value {
        Spades,
        Hearts,
        Diamonds,
        Clubs,
    }
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        enum_field: Value,
    }
    let value = Test {
        enum_field: Value::Hearts,
    };
    generate_test_case(schema, value, "enum");
}

fn generate_avro_test_case_union() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "union_field", "type": [
                "string",
                "int"
                ]
            }
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        union_field: i32,
    }
    let value = Test {
        union_field: 123456,
    };
    generate_test_case(schema, value, "union");
}

fn generate_avro_test_case_array() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "array_field", "type": "array", "items" : "string"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        array_field: Vec<String>,
    }
    let value = Test {
        array_field: vec![
            "hello".to_string(),
            "vector".to_string(),
            "avro".to_string(),
            "codec".to_string(),
        ],
    };
    generate_test_case(schema, value, "array");
}

fn generate_avro_test_case_map() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "map_field", "type": "map", "values" : "long","default": {}}
        ]
    }
    "#;
    use std::collections::HashMap;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        map_field: HashMap<String, i64>,
    }
    let mut scores = HashMap::new();
    scores.insert(String::from("Blue"), 10i64);
    let value = Test { map_field: scores };
    generate_test_case(schema, value, "map");
}

fn generate_avro_test_case_record() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "name", "type": "string"},
            {"name": "age", "type": "int"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        name: String,
        age: i32,
    }
    let value = Test {
        name: "John".to_string(),
        age: 23,
    };
    generate_test_case(schema, value, "record");
}

#[allow(unused)]
fn generate_avro_test_case_date() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "date_field", "type": "int", "logicalType": "date"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        date_field: i32,
    }
    let value = Test { date_field: 19646 };
    generate_test_case(schema, value, "date");
}

#[allow(unused)]
fn generate_avro_test_case_decimal_var() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "decimal_var_field", "type": "bytes", "logicalType": "decimal","precision": 10,"scale": 3}
        ]
    }
    "#;

    let record = Value::Record(vec![(
        "decimal_var_field".into(),
        Value::Decimal(Decimal::from([
            249, 33, 74, 206, 142, 64, 190, 170, 17, 153,
        ])),
    )]);
    generate_test_case_from_value(schema, record, "decimal_var");
}

#[allow(unused)]
fn generate_avro_test_case_time_millis() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "time_millis_field", "type": "int", "logicalType": "time-millis"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        time_millis_field: i32,
    }
    let value = Test {
        time_millis_field: 59820123,
    };
    generate_test_case(schema, value, "time_millis");
}

fn generate_avro_test_case_time_micros() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "time_micros_field", "type": "long", "logicalType": "time-micros"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        time_micros_field: i64,
    }
    let value: Test = Test {
        time_micros_field: 59820123456i64,
    };
    generate_test_case(schema, value, "time_micros");
}

fn generate_avro_test_case_timestamp_millis() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "timestamp_millis_field", "type": "long", "logicalType": "timestamp-millis"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        timestamp_millis_field: i64,
    }
    let value = Test {
        timestamp_millis_field: 1697445291056i64,
    };
    generate_test_case(schema, value, "timestamp_millis");
}

fn generate_avro_test_case_timestamp_micros() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "timestamp_micros_field", "type": "long", "logicalType": "timestamp-micros"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        timestamp_micros_field: i64,
    }
    let value = Test {
        timestamp_micros_field: 1697445291056567i64,
    };
    generate_test_case(schema, value, "timestamp_micros");
}

fn generate_avro_test_case_local_timestamp_millis() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "local_timestamp_millis_field", "type": "long", "logicalType": "local-timestamp-millis"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        local_timestamp_millis_field: i64,
    }
    let value = Test {
        local_timestamp_millis_field: 1697445291056i64,
    };
    generate_test_case(schema, value, "local-timestamp_millis");
}

fn generate_avro_test_case_local_timestamp_micros() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "local_timestamp_micros_field", "type": "long", "logicalType": "local-timestamp-micros"}
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        local_timestamp_micros_field: i64,
    }
    let value = Test {
        local_timestamp_micros_field: 1697445291056567i64,
    };
    generate_test_case(schema, value, "local-timestamp_micros");
}

fn generate_avro_test_case_uuid() {
    let schema = r#"
    {
        "type": "record",
        "name": "test",
        "fields": [
            {"name": "uuid_field", "type": "string",
              "logicalType": "uuid"
            }
        ]
    }
    "#;
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct Test {
        uuid_field: String,
    }
    let value = Test {
        uuid_field: "550e8400-e29b-41d4-a716-446655440000".into(),
    };
    generate_test_case(schema, value, "uuid");
}

fn generate_test_case<S: Serialize>(schema: &str, value: S, filename: &str) {
    let value = apache_avro::to_value(value).unwrap();
    generate_test_case_from_value(schema, value, filename);
}

fn generate_test_case_from_value(schema: &str, value: Value, filename: &str) {
    let schema = Schema::parse_str(schema).unwrap();

    let value = value.resolve(&schema).unwrap();
    let bytes = apache_avro::to_avro_datum(&schema, value).unwrap();

    let mut schema_file = File::create(format!("{FIXTURES_PATH}/{filename}.avsc")).unwrap();
    let mut avro_file = File::create(format!("{FIXTURES_PATH}/{filename}.avro")).unwrap();
    schema_file
        .write_all(schema.canonical_form().as_bytes())
        .unwrap();
    avro_file.write_all(&bytes).unwrap();
}

fn main() {
    if !PathBuf::from(FIXTURES_PATH).is_dir() {
        panic!("dir {FIXTURES_PATH} not exist\n");
    }
    generate_avro_test_case_array();
    generate_avro_test_case_boolean();
    generate_avro_test_case_bytes();
    generate_avro_test_case_double();
    generate_avro_test_case_enum();
    generate_avro_test_case_float();
    generate_avro_test_case_int();
    generate_avro_test_case_long();
    generate_avro_test_case_map();
    generate_avro_test_case_record();
    generate_avro_test_case_string();
    generate_avro_test_case_time_micros();
    generate_avro_test_case_timestamp_micros();
    generate_avro_test_case_timestamp_millis();
    generate_avro_test_case_local_timestamp_micros();
    generate_avro_test_case_local_timestamp_millis();
    generate_avro_test_case_union();
    generate_avro_test_case_uuid();
}
