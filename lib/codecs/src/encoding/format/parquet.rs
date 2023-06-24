use core::panic;
use std::{io, sync::Arc};

use bytes::{BufMut, BytesMut};
use parquet::{
    basic::{LogicalType, Repetition},
    column::writer::{ColumnWriter::*, ColumnWriterImpl},
    data_type::DataType,
    errors::ParquetError,
    file::{properties::WriterProperties, writer::SerializedFileWriter},
    schema::{
        parser::parse_message_type,
        types::{BasicTypeInfo, ColumnDescriptor, Type, TypePtr},
    },
};
use serde::{Deserialize, Serialize};
use snafu::*;
use tokio_util::codec::Encoder;

use vector_config::configurable_component;
use vector_core::{
    config,
    event::{Event, Value},
    schema,
};

use crate::encoding::BuildError;

/// Errors that can occur during Parquet serialization.
#[derive(Debug, Snafu)]
pub enum ParquetSerializerError {
    #[snafu(display(r#"Event does not contain required field. field = "{}""#, field))]
    MissingField {
        field: String,
    },
    #[snafu(display(
        r#"Event contains a value with an invalid type. field = "{}" type = "{}" expected type = "{}""#,
        field,
        actual_type,
        expected_type
    ))]
    InvalidValueType {
        field: String,
        actual_type: String,
        expected_type: String,
    },
    #[snafu(display("Failed to write. error: {}", error))]
    ParquetError {
        error: ParquetError,
    },
    // TODO: Can this actually happen?
    IoError {
        source: io::Error,
    },
}

impl ParquetSerializerError {
    fn invalid_type(
        desc: &ColumnDescriptor,
        value: &Value,
        expected: &str,
    ) -> ParquetSerializerError {
        ParquetSerializerError::InvalidValueType {
            field: desc.name().to_string(),
            actual_type: value.kind_str().to_string(),
            expected_type: expected.to_string(),
        }
    }
}

impl From<ParquetError> for ParquetSerializerError {
    fn from(error: ParquetError) -> Self {
        Self::ParquetError { error }
    }
}

impl From<io::Error> for ParquetSerializerError {
    fn from(source: io::Error) -> Self {
        Self::IoError { source }
    }
}

/// Config used to build a `ParquetSerializer`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParquetSerializerConfig {
    /// Options for the Parquet serializer.
    pub parquet: ParquetSerializerOptions,
}

impl ParquetSerializerConfig {
    /// Creates a new `ParquetSerializerConfig`.
    pub const fn new(schema: String) -> Self {
        Self {
            parquet: ParquetSerializerOptions { schema },
        }
    }

    /// Build the `ParquetSerializerConfig` from this configuration.
    pub fn build(&self) -> Result<ParquetSerializer, BuildError> {
        let schema = parse_message_type(&self.parquet.schema)
            .map_err(|error| format!("Failed building Parquet serializer: {}", error))?;
        self.validate_logical_schema(&schema)
            .map_err(|error| format!("Failed building Parquet serializer: {}", error))?;
        Ok(ParquetSerializer {
            schema: Arc::new(schema),
        })
    }

    /// The data type of events that are accepted by `ParquetSerializer`.
    pub fn input_type(&self) -> config::DataType {
        config::DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // TODO: Convert the Parquet schema to a vector schema requirement.
        // NOTE: This isn't yet doable. We don't have meanings to
        // to specify for requirement.
        schema::Requirement::empty()
    }

    fn validate_logical_schema(&self, schema: &Type) -> Result<(), String> {
        let info = schema.get_basic_info();
        match info.logical_type() {
            // Validate LIST types
            Some(LogicalType::List) => {
                if info.has_repetition() && info.repetition() == Repetition::REPEATED {
                    return Err(format!(
                        "Invalid repetition for LIST type. repetition = {:?}",
                        info.repetition()
                    ));
                }
                if schema.get_fields().len() != 1 {
                    return Err(format!(
                        "Invalid LIST type. LIST type must have a single child, found {}.",
                        schema.get_fields().len()
                    ));
                }

                let list = schema.get_fields().get(0).expect("must have a child");
                let info = list.get_basic_info();
                if !info.has_repetition() || info.repetition() != Repetition::REPEATED {
                    return Err(format!(
                        "Invalid repetition for child of LIST type. repetition = {:?}",
                        if info.has_repetition() {
                            info.repetition()
                        } else {
                            Repetition::REQUIRED
                        }
                    ));
                }
                if list.get_fields().len() != 1 {
                    return Err(format!(
                        "Invalid LIST type. Child of LIST type must have a single child, found {}.",
                        list.get_fields().len()
                    ));
                }

                let element = list.get_fields().get(0).expect("must have a child");
                let info = element.get_basic_info();
                if info.has_repetition() && info.repetition() == Repetition::REPEATED {
                    return Err(format!(
                        "Invalid repetition for element of LIST type. repetition = {:?}",
                        info.repetition()
                    ));
                }

                self.validate_logical_schema(element)?;
            }
            _ => {
                for field in schema.get_fields() {
                    self.validate_logical_schema(field)?;
                }
            }
        }
        Ok(())
    }
}

/// Options for the Parquet serializer.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct ParquetSerializerOptions {
    /// The Parquet schema.
    #[configurable(metadata(docs::examples = r#"message test {
        required group data {
            required binary name;
            repeated int64 values;
        }
    }"#))]
    pub schema: String,
}

/// Serializer that converts `Vec<Event>` to bytes using the Apache Parquet format.
#[derive(Debug, Clone)]
pub struct ParquetSerializer {
    schema: TypePtr,
}

impl ParquetSerializer {
    /// Creates a new `ParquetSerializer`.
    pub const fn new(schema: TypePtr) -> Self {
        Self { schema }
    }
}

impl ParquetSerializer {
    fn process<T: DataType>(
        &self,
        events: &[Event],
        desc: &ColumnDescriptor,
        extractor: impl Fn(&Value) -> Result<<T as DataType>::T, ParquetSerializerError>,
        writer: &mut ColumnWriterImpl<T>,
    ) -> Result<(), ParquetSerializerError> {
        let mut column = Column::<<T as DataType>::T, _>::new(&*self.schema, desc, extractor);
        column.extract_column(events)?;
        let written_values = writer.write_batch(
            &column.values,
            column.def_levels.as_ref().map(|vec| vec.as_slice()),
            column.rep_levels.as_ref().map(|vec| vec.as_slice()),
        )?;

        assert_eq!(written_values, column.values.len());
        Ok(())
    }
}

impl Encoder<Vec<Event>> for ParquetSerializer {
    type Error = vector_common::Error;

    /// Builds columns from events and writes them to the writer.
    ///
    /// Expects that all events satisfy the schema, else whole batch can fail.
    fn encode(&mut self, events: Vec<Event>, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        // Encode events
        let props = Arc::new(WriterProperties::builder().build());
        let mut parquet_writer =
            SerializedFileWriter::new(buffer.writer(), self.schema.clone(), props)?;

        let mut row_group_writer = parquet_writer.next_row_group()?;
        while let Some(mut column_writer) = row_group_writer.next_column()? {
            match column_writer.untyped() {
                BoolColumnWriter(ref mut writer) => {
                    let desc = writer.get_descriptor().clone();
                    self.process(
                        &events,
                        &desc,
                        |value| match value {
                            Value::Boolean(value) => Ok(*value),
                            _ => Err(ParquetSerializerError::invalid_type(
                                &desc, value, "boolean",
                            )),
                        },
                        writer,
                    )?
                }
                Int64ColumnWriter(writer) => {
                    let desc = writer.get_descriptor().clone();
                    self.process(
                        &events,
                        &desc,
                        |value| match value {
                            Value::Integer(value) => Ok(*value),
                            _ => Err(ParquetSerializerError::invalid_type(
                                &desc, value, "integer",
                            )),
                        },
                        writer,
                    )?
                }
                DoubleColumnWriter(writer) => {
                    let desc = writer.get_descriptor().clone();
                    self.process(
                        &events,
                        &desc,
                        |value| match value {
                            Value::Float(value) => Ok(value.into_inner()),
                            _ => Err(ParquetSerializerError::invalid_type(&desc, value, "float")),
                        },
                        writer,
                    )?
                }
                ByteArrayColumnWriter(writer) => {
                    let desc = writer.get_descriptor().clone();
                    self.process(
                        &events,
                        &desc,
                        |value| match value {
                            Value::Bytes(value) => Ok(value.clone().into()),
                            _ => Err(ParquetSerializerError::invalid_type(&desc, value, "string")),
                        },
                        writer,
                    )?
                }
                FixedLenByteArrayColumnWriter(_) => {
                    panic!("Fixed len byte array is not supported.");
                }
                Int32ColumnWriter(_) => panic!("Int32 is not supported."),
                Int96ColumnWriter(_) => panic!("Int96 is not supported."),
                FloatColumnWriter(_) => panic!("Float32 is not supported."),
            }
            column_writer.close()?;
        }

        row_group_writer.close()?;
        parquet_writer.close()?;

        Ok(())
    }
}

struct Column<'a, T, F: Fn(&Value) -> Result<T, ParquetSerializerError>> {
    levels: Vec<&'a Type>,
    extract: F,
    values: Vec<T>,
    // If present encodes definition level. From 0 to column.max_def_level() inclusive.
    // With any value bellow max encoding null on that level.
    // One thing to keep in mind, if a column is required on some "level" then that level is not counted here.
    // This is needed when values are optional.
    // In case of null, that value is skipped in values.
    def_levels: Option<Vec<i16>>,
    // If present encodes repetition level.
    // From 0 to column.max_rep_level() inclusive. With 0 starting a new record and any value bellow max encoding
    // starting new list at that level. With max level just adding element to leaf list.
    // This is needed when values are repeated. Where that repetition can have multiple nested levels.
    rep_levels: Option<Vec<i16>>,
}

impl<'a, T, F: Fn(&Value) -> Result<T, ParquetSerializerError>> Column<'a, T, F> {
    fn new(schema: &'a Type, column: &'a ColumnDescriptor, extract: F) -> Self {
        let mut levels = vec![schema];
        for part in column.path().parts() {
            match &levels[levels.len() - 1] {
                Type::GroupType { fields, .. } => {
                    let field = fields
                        .iter()
                        .find(|field| field.name() == part)
                        .expect("Field not found in schema.");
                    levels.push(field);
                }
                Type::PrimitiveType { .. } => unreachable!(),
            }
        }

        let def_levels = if levels.iter().any(|ty| ty.is_optional()) {
            Some(Vec::new())
        } else {
            None
        };

        let rep_levels = if levels.iter().any(|ty| {
            let info = ty.get_basic_info();
            info.has_repetition() && info.repetition() == Repetition::REPEATED
        }) {
            Some(Vec::new())
        } else {
            None
        };

        Self {
            levels,
            extract,
            values: Vec::new(),
            def_levels,
            rep_levels,
        }
    }

    fn extract_column(&mut self, events: &[Event]) -> Result<(), ParquetSerializerError> {
        for event in events {
            match event {
                Event::Log(log) => {
                    self.extract_value(log.value(), Level::root())?;
                }
                Event::Trace(trace) => {
                    self.extract_value(trace.value(), Level::root())?;
                }
                Event::Metric(_) => {
                    panic!("Metrics are not supported.");
                }
            }
        }
        Ok(())
    }

    /// Will push at least one value, or error.
    fn extract_value(&mut self, value: &Value, level: Level) -> Result<(), ParquetSerializerError> {
        if let Some(part) = self.levels.get(level.level) {
            let sub = match value {
                Value::Object(object) => object.get(part.name()),
                Value::Null => None,
                // Invalid type, error
                value => {
                    // TODO: Check out if field paths for errors make sense
                    return Err(ParquetSerializerError::InvalidValueType {
                        field: self.path(level),
                        actual_type: value.kind_str().to_string(),
                        expected_type: "object".to_string(),
                    });
                }
            };

            self.process_value(sub, level)
        } else if matches!(value, Value::Null) {
            self.push_value(None, level.undefine());
            Ok(())
        } else {
            let value = (self.extract)(value)?;
            self.push_value(Some(value), level);
            Ok(())
        }
    }

    /// Will push at least one value, or error.
    fn process_value(
        &mut self,
        value: Option<&Value>,
        level: Level,
    ) -> Result<(), ParquetSerializerError> {
        let part = self
            .levels
            .get(level.level)
            .expect("We should have checked this before hand.");
        match value {
            Some(Value::Null) | None if part.is_optional() => {
                self.push_value(None, level);
                Ok(())
            }
            // Illegal null, error
            Some(Value::Null) | None => Err(ParquetSerializerError::MissingField {
                field: self.path(level),
            }),
            Some(value) => {
                let info = part.get_basic_info();
                match info.logical_type() {
                    Some(LogicalType::List) => {
                        if let Value::Array(array) = value {
                            let list_level = level.descend(info);
                            let element_level = list_level.descend_repeated();

                            if array.is_empty() {
                                self.push_value(None, list_level);
                            } else {
                                let mut now = element_level;
                                for element in array {
                                    self.process_value(Some(element), now)?;
                                    now = now.next();
                                }
                            }

                            Ok(())
                        } else {
                            return Err(ParquetSerializerError::InvalidValueType {
                                field: self.path(level),
                                actual_type: value.kind_str().to_string(),
                                expected_type: "array".to_string(),
                            });
                        }
                    }
                    _ => self.extract_flat(value, level.descend(info)),
                }
            }
        }
    }

    /// Will push at least one value, or error.
    fn extract_flat(&mut self, value: &Value, level: Level) -> Result<(), ParquetSerializerError> {
        match value {
            Value::Array(array) if level.repeated => {
                if array.is_empty() {
                    self.push_value(None, level.undefine());
                } else {
                    let mut now = level;
                    for value in array {
                        self.extract_value(value, now)?;
                        now = now.next();
                    }
                }

                Ok(())
            }
            _ => self.extract_value(value, level),
        }
    }

    fn push_value(&mut self, value: Option<T>, level: Level) {
        if let Some(rep_levels) = &mut self.rep_levels {
            rep_levels.push(level.start_rep_level);
        }
        if let Some(def_levels) = &mut self.def_levels {
            def_levels.push(level.def_level);
        }
        if let Some(value) = value {
            self.values.push(value);
        }
    }

    fn path(&self, level: Level) -> String {
        let mut path = String::new();
        for level in &self.levels[1..level.level] {
            path.push_str(level.name());
            path.push('.');
        }
        path.push_str(self.levels[level.level].name());
        path
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct Level {
    start_rep_level: i16,
    rep_level: i16,
    def_level: i16,
    level: usize,
    // If this level can be null
    optional: bool,
    // If this level is repeated
    repeated: bool,
}

impl Level {
    fn root() -> Self {
        Self {
            start_rep_level: 0,
            rep_level: 0,
            def_level: 0,
            level: 1,
            optional: false,
            repeated: false,
        }
    }

    fn next(self) -> Self {
        assert!(self.repeated);
        Self {
            start_rep_level: self.rep_level,
            ..self
        }
    }

    /// Descend implies that the level is defined.
    fn descend(self, info: &BasicTypeInfo) -> Self {
        if info.has_repetition() {
            match info.repetition() {
                Repetition::OPTIONAL => self.descend_optional(),
                Repetition::REPEATED => self.descend_repeated(),
                Repetition::REQUIRED => self.descend_required(),
            }
        } else {
            self.descend_required()
        }
    }

    fn descend_required(self) -> Self {
        Self {
            level: self.level + 1,
            repeated: false,
            optional: false,
            ..self
        }
    }

    fn descend_optional(self) -> Self {
        Self {
            def_level: self.def_level + 1,
            level: self.level + 1,
            repeated: false,
            optional: true,
            ..self
        }
    }

    fn descend_repeated(self) -> Self {
        Self {
            rep_level: self.rep_level + 1,
            def_level: self.def_level + 1,
            level: self.level + 1,
            repeated: true,
            optional: true,
            ..self
        }
    }

    /// Undefines by one level
    fn undefine(self) -> Self {
        Self {
            def_level: self.def_level - 1,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use parquet::{
        column::reader::{ColumnReader, ColumnReaderImpl},
        file::reader::*,
        file::serialized_reader::SerializedFileReader,
        schema::parser::parse_message_type,
    };
    use similar_asserts::assert_eq;
    use std::panic;
    use std::{collections::HashSet, sync::Arc};
    use vector_core::event::LogEvent;
    use vrl::value::btreemap;

    macro_rules! log_event {
        ($($key:expr => $value:expr),*  $(,)?) => {
            #[allow(unused_variables)]
            {
                let mut event = Event::Log(LogEvent::default());
                let log = event.as_mut_log();
                $(
                    log.insert($key, $value);
                )*
                event
            }
        };
    }

    fn assert_column<T: DataType>(
        count: usize,
        expect_values: &[<T as DataType>::T],
        expect_rep_levels: Option<&[i16]>,
        expect_def_levels: Option<&[i16]>,
        mut column_reader: ColumnReaderImpl<T>,
    ) where
        <T as DataType>::T: Default,
    {
        let mut values = Vec::new();
        values.resize(count, <T as DataType>::T::default());
        let mut def_levels = Vec::new();
        def_levels.resize(count, 0);
        let mut rep_levels = Vec::new();
        rep_levels.resize(count, 0);
        let (read, level) = column_reader
            .read_batch(
                count,
                Some(def_levels.as_mut_slice()).filter(|_| expect_def_levels.is_some()),
                Some(rep_levels.as_mut_slice()).filter(|_| expect_rep_levels.is_some()),
                &mut values,
            )
            .unwrap();

        assert_eq!(level, count);
        assert_eq!(&values[..read], expect_values);
        if expect_rep_levels.is_some() {
            assert_eq!(rep_levels, expect_rep_levels.unwrap());
        }
        if expect_def_levels.is_some() {
            assert_eq!(def_levels, expect_def_levels.unwrap());
        }
    }

    fn validate(
        schema: &str,
        events: Vec<Event>,
        num_columns: usize,
        validate: impl Fn(usize, &str, &dyn RowGroupReader),
    ) {
        let schema = Arc::new(parse_message_type(schema).unwrap());
        let mut encoder = ParquetSerializer::new(schema);

        let mut buffer = BytesMut::new();
        encoder.encode(events, &mut buffer).unwrap();

        let reader = SerializedFileReader::new(buffer.freeze()).unwrap();

        let parquet_metadata = reader.metadata();
        assert_eq!(parquet_metadata.num_row_groups(), 1);

        let row_group_reader = reader.get_row_group(0).unwrap();
        assert_eq!(row_group_reader.num_columns(), num_columns);

        let metadata = row_group_reader.metadata();
        let mut visited = HashSet::new();
        for (i, column) in metadata.columns().iter().enumerate() {
            let path = column.column_path().string();
            assert!(visited.insert(path.clone()));
            validate(i, &path, row_group_reader.as_ref());
        }

        assert_eq!(visited.len(), num_columns);
    }

    #[test]
    fn test_serialize() {
        let message_type = r#"
            message test {
                required group a {
                    required boolean b;
                    optional int64 c;
                }
                optional group d {
                    optional int64 e;
                }
                required group f {
                    repeated int64 g;
                }
                required binary h;
                repeated group i {
                    required int64 j;
                    repeated double k;
                }
            }
            "#;

        let events = vec![
            log_event! {
            "a.b" => true,
            "a.c" => 2,
            "d.e" => 3,
            "f.g" => vec![4, 5],
            "h" => "hello",
            "i" => vec![btreemap! {
                    "j" => 6,
                    "k" => vec![7.0, 8.0]
                }]
            },
            log_event! {
            "a.b" => false,
            "f" => Value::Object(Default::default()),
            "h" => "world",
            "i" => vec![
                btreemap! {
                    "j" => 9,
                    "k" => vec![10.0]
                }, btreemap! {
                    "j" => 11,
                }]
            },
        ];

        validate(
            message_type,
            events,
            7,
            |i, path, row_group_reader| match path {
                "a.b" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::BoolColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(2, &[true, false], None, None, reader);
                }
                "a.c" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::Int64ColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(2, &[2], None, Some(&[1, 0]), reader);
                }
                "d.e" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::Int64ColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(2, &[3], None, Some(&[2, 0]), reader);
                }
                "f.g" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::Int64ColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(3, &[4, 5], Some(&[0, 1, 0]), Some(&[1, 1, 0]), reader);
                }
                "h" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::ByteArrayColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(
                        2,
                        &[Bytes::from("hello").into(), Bytes::from("world").into()],
                        None,
                        None,
                        reader,
                    );
                }
                "i.j" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::Int64ColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(3, &[6, 9, 11], Some(&[0, 0, 1]), Some(&[1, 1, 1]), reader);
                }
                "i.k" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::DoubleColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(
                        4,
                        &[7.0, 8.0, 10.0],
                        Some(&[0, 2, 0, 1]),
                        Some(&[2, 2, 2, 1]),
                        reader,
                    );
                }
                _ => panic!("Unexpected column"),
            },
        );
    }

    #[test]
    fn test_value_null() {
        let message_type = r#"
            message test {
                optional group geo{
                    optional binary city_name (UTF8);  
                }            
            }
            "#;

        let events = vec![
            log_event! {
                "geo.city_name" => "hello",
            },
            log_event! {
                "geo.city_name" => Value::Null,
            },
        ];

        validate(
            message_type,
            events,
            1,
            |i, path, row_group_reader| match path {
                "geo.city_name" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::ByteArrayColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(
                        2,
                        &[Bytes::from("hello").into()],
                        None,
                        Some(&[2, 1]),
                        reader,
                    );
                }
                _ => panic!("Unexpected column"),
            },
        );
    }

    #[test]
    fn test_value_null_stack_optional() {
        let message_type = r#"
            message test {
                optional group a{
                    optional group b{
                        optional boolean c;
                    }
                }            
            }
            "#;

        let events = vec![
            log_event! {},
            log_event! {"a" => Value::Null},
            log_event! {"a.b" => Value::Null},
            log_event! {"a.b.c" => Value::Null},
            log_event! {"a.b.c" => Value::Boolean(true)},
        ];

        validate(
            message_type,
            events,
            1,
            |i, path, row_group_reader| match path {
                "a.b.c" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::BoolColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(5, &[true], None, Some(&[0, 0, 1, 2, 3]), reader);
                }
                _ => panic!("Unexpected column"),
            },
        );
    }

    #[test]
    fn test_value_null_repeated_optional() {
        let message_type = r#"
            message test {
                repeated group a{
                    optional boolean b;
                }            
            }
            "#;

        let events = vec![
            log_event! {},
            log_event! {"a" => Value::Null},
            log_event! {"a.b" => Value::Null},
            log_event! {"a.b" => Value::Boolean(false)},
            log_event! {"a" => vec![
                btreemap! { "b" => Value::Null },
                btreemap! { "b" => Value::Boolean(true) }
            ]},
        ];
        validate(
            message_type,
            events,
            1,
            |i, path, row_group_reader| match path {
                "a.b" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::BoolColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(
                        6,
                        &[false, true],
                        Some(&[0, 0, 0, 0, 0, 1]),
                        Some(&[0, 0, 1, 2, 1, 2]),
                        reader,
                    );
                }
                _ => panic!("Unexpected column"),
            },
        );
    }

    #[test]
    fn test_value_null_repeated() {
        let message_type = r#"
            message test {
                repeated boolean a;
            }
            "#;

        let events = vec![
            log_event! {},
            log_event! {"a" => Value::Null},
            log_event! {"a" => Value::Boolean(false)},
            log_event! {"a" => vec![
                Value::Null,
                Value::Boolean(true),
                Value::Null,
            ]},
        ];

        validate(
            message_type,
            events,
            1,
            |i, path, row_group_reader| match path {
                "a" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::BoolColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(
                        6,
                        &[false, true],
                        Some(&[0, 0, 0, 0, 1, 1]),
                        Some(&[0, 0, 1, 0, 1, 0]),
                        reader,
                    );
                }
                _ => panic!("Unexpected column"),
            },
        );
    }

    #[test]
    fn test_value_empty_repeated() {
        let message_type = r#"
            message test {
                repeated boolean a;
            }
            "#;

        let events = vec![log_event! {"a" => Vec::<Value>::new()}];

        validate(
            message_type,
            events,
            1,
            |i, path, row_group_reader| match path {
                "a" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::BoolColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(1, &[], Some(&[0]), Some(&[0]), reader);
                }
                _ => panic!("Unexpected column"),
            },
        );
    }

    #[test]
    fn test_repeated() {
        let message_type = r#"
            message test {
                repeated group answer {
                    optional binary name (UTF8);
                    optional INT64 ttl;
                }           
            }
            "#;

        let events = vec![log_event! {
            "answer" => vec![
            btreemap! {
                "name" => "test1",
                "ttl" => 0,
            }, btreemap! {
                "name" => "test2",
                "ttl" => 3600,
            }]
        }];

        validate(
            message_type,
            events,
            2,
            |i, path, row_group_reader| match path {
                "answer.name" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::ByteArrayColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(
                        2,
                        &[Bytes::from("test1").into(), Bytes::from("test2").into()],
                        Some(&[0, 1]),
                        Some(&[2, 2]),
                        reader,
                    );
                }
                "answer.ttl" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::Int64ColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(2, &[0, 3600], Some(&[0, 1]), Some(&[2, 2]), reader);
                }
                _ => panic!("Unexpected column"),
            },
        );
    }

    #[test]
    fn test_list() {
        let message_type = r#"
            message test {
                optional group answers (LIST){
                    repeated group list {
                        optional boolean element;
                    }
                }
            }
            "#;

        let events = vec![
            log_event! {},
            log_event! {"answers" => Value::Null},
            log_event! {"answers" => Vec::<Value>::new()},
            log_event! {"answers" => vec![
                Value::Null,
                Value::Boolean(true),
                Value::Null,
            ]},
        ];

        validate(
            message_type,
            events,
            1,
            |i, path, row_group_reader| match path {
                "answers.list.element" => {
                    let reader = match row_group_reader.get_column_reader(i).unwrap() {
                        ColumnReader::BoolColumnReader(r) => r,
                        _ => panic!("Wrong column type"),
                    };
                    assert_column(
                        6,
                        &[true],
                        Some(&[0, 0, 0, 0, 1, 1]),
                        Some(&[0, 0, 1, 2, 3, 2]),
                        reader,
                    );
                }
                _ => panic!("Unexpected column"),
            },
        );
    }

    #[test]
    fn illegal_list_scheme() {
        let config = ParquetSerializerConfig {
            parquet: ParquetSerializerOptions {
                schema: r#"
            message test {
                optional group answers (LIST){
                    optional group list {
                        optional boolean element;
                    }
                }
            }
            "#
                .to_string(),
            },
        };

        assert!(config.build().is_err());
    }
}
