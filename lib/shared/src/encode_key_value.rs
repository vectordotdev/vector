use serde::ser::*;
use std::collections::BTreeMap;
use std::fmt::{self, Write};
use std::result::Result;

#[derive(Debug, snafu::Snafu)]
pub enum EncodingError {
    #[snafu(display("Key is not String."))]
    KeyNotString,
    #[snafu(display("Encoding error: {}.", msg))]
    Other { msg: String },
}

impl Error for EncodingError {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Self::Other {
            msg: msg.to_string(),
        }
    }
}

pub fn encode<'a>(
    input: BTreeMap<String, impl Serialize>,
    fields: &[String],
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
    flatten_boolean: bool,
) -> Result<String, EncodingError> {
    let mut output = String::new();

    let mut input = flatten(input, '.')?;

    for field in fields.iter() {
        match (input.remove(field), flatten_boolean) {
            (Some(Data::Boolean(false)), true) => (),
            (Some(Data::Boolean(true)), true) => {
                encode_string(&mut output, field);
                output.write_str(field_delimiter).unwrap();
            }
            (Some(value), _) => {
                encode_field(&mut output, field, &value.to_string(), key_value_delimiter);
                output.write_str(field_delimiter).unwrap();
            }
            (None, _) => (),
        };
    }

    for (key, value) in input.iter() {
        match (value, flatten_boolean) {
            (Data::Boolean(false), true) => (),
            (Data::Boolean(true), true) => {
                encode_string(&mut output, key);
                output.write_str(field_delimiter).unwrap();
            }
            (_, _) => {
                encode_field(&mut output, key, &value.to_string(), key_value_delimiter);
                output.write_str(field_delimiter).unwrap();
            }
        };
    }

    if output.ends_with(field_delimiter) {
        output.truncate(output.len() - field_delimiter.len())
    }

    Ok(output)
}

fn flatten<'a>(
    input: impl IntoIterator<Item = (String, impl Serialize)> + 'a,
    separator: char,
) -> Result<BTreeMap<String, Data>, EncodingError> {
    let mut map = BTreeMap::new();
    for (key, value) in input {
        value.serialize(KeyValueSerializer::new(key, separator, &mut map))?;
    }
    Ok(map)
}

fn encode_field<'a>(output: &mut String, key: &str, value: &str, key_value_delimiter: &'a str) {
    encode_string(output, key);
    output.write_str(key_value_delimiter).unwrap();
    encode_string(output, value);
}

fn encode_string(output: &mut String, str: &str) {
    let needs_quoting = str.chars().any(char::is_whitespace);

    if needs_quoting {
        output.write_char('"').unwrap();
    }

    for c in str.chars() {
        match c {
            '\\' => output.write_str(r#"\\"#).unwrap(),
            '"' => output.write_str(r#"\""#).unwrap(),
            '\n' => output.write_str(r#"\\n"#).unwrap(),
            _ => output.write_char(c).unwrap(),
        }
    }

    if needs_quoting {
        output.write_char('"').unwrap();
    }
}

enum Data {
    None,
    Boolean(bool),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    I128(i128),
    U128(u128),
    Char(char),
    String(String),
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Data::None => write!(f, "null"),
            Data::Boolean(val) => write!(f, "{}", val),
            Data::I64(val) => write!(f, "{}", val),
            Data::U64(val) => write!(f, "{}", val),
            Data::F32(val) => write!(f, "{}", val),
            Data::F64(val) => write!(f, "{}", val),
            Data::I128(val) => write!(f, "{}", val),
            Data::U128(val) => write!(f, "{}", val),
            Data::Char(val) => write!(f, "{}", val),
            Data::String(val) => write!(f, "{}", val),
        }
    }
}

struct KeyValueSerializer<'a> {
    key: String,
    separator: char,
    output: &'a mut BTreeMap<String, Data>,
}

impl<'a> KeyValueSerializer<'a> {
    fn new(key: String, separator: char, output: &'a mut BTreeMap<String, Data>) -> Self {
        Self {
            key,
            separator,
            output,
        }
    }

    fn indexed(self) -> IndexedKeyValueSerializer<'a> {
        IndexedKeyValueSerializer {
            index: 0,
            ser: self,
        }
    }

    fn keyed(self) -> KeyedKeyValueSerializer<'a> {
        KeyedKeyValueSerializer {
            key: None,
            ser: self,
        }
    }

    fn descend(mut self, child: impl fmt::Display) -> Self {
        self.key.push(self.separator);
        write!(&mut self.key, "{}", child).expect("Shouldn't be reachable.");
        self
    }

    fn child<'b>(&'b mut self, child: impl fmt::Display) -> KeyValueSerializer<'b> {
        KeyValueSerializer {
            key: format!("{}{}{}", self.key, self.separator, child),
            separator: self.separator,
            output: self.output,
        }
    }

    fn process(self, data: Data) -> Result<(), EncodingError> {
        self.output.insert(self.key, data);
        Ok(())
    }
}

impl<'a> Serializer for KeyValueSerializer<'a> {
    type Ok = ();
    type Error = EncodingError;

    type SerializeSeq = IndexedKeyValueSerializer<'a>;
    type SerializeTuple = IndexedKeyValueSerializer<'a>;
    type SerializeTupleStruct = IndexedKeyValueSerializer<'a>;
    type SerializeTupleVariant = IndexedKeyValueSerializer<'a>;
    type SerializeMap = KeyedKeyValueSerializer<'a>;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.process(Data::Boolean(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.process(Data::I64(v as i64))
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.process(Data::I64(v as i64))
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.process(Data::I64(v as i64))
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.process(Data::I64(v as i64))
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.process(Data::U64(v as u64))
    }
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.process(Data::U64(v as u64))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.process(Data::U64(v as u64))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.process(Data::U64(v as u64))
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.process(Data::F32(v))
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.process(Data::F64(v))
    }

    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.process(Data::I128(v))
    }

    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        self.process(Data::U128(v))
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        self.process(Data::Char(v))
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.process(Data::String(v.to_owned()))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.process(Data::String(String::from_utf8_lossy(v).into_owned()))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.process(Data::None)
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.process(Data::None)
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.descend(name).process(Data::None)
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.descend(name).descend(variant).process(Data::None)
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self.descend(name))
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self.descend(name).descend(variant))
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(self.indexed())
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(self.indexed())
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(self.descend(name).indexed())
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(self.descend(name).descend(variant).indexed())
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(self.keyed())
    }

    fn serialize_struct(
        self,
        name: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(self.descend(name))
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(self.descend(name).descend(variant))
    }
}

impl<'a> SerializeStruct for KeyValueSerializer<'a> {
    type Ok = ();
    type Error = EncodingError;
    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self.child(key))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> SerializeStructVariant for KeyValueSerializer<'a> {
    type Ok = ();
    type Error = EncodingError;
    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self.child(key))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

struct IndexedKeyValueSerializer<'a> {
    index: usize,
    ser: KeyValueSerializer<'a>,
}

impl<'a> IndexedKeyValueSerializer<'a> {
    fn process<T: ?Sized + Serialize>(&mut self, data: &T) -> Result<(), EncodingError> {
        let key = self.index;
        self.index += 1;
        data.serialize(self.ser.child(key))
    }
}

impl<'a> SerializeTuple for IndexedKeyValueSerializer<'a> {
    type Ok = ();
    type Error = EncodingError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.process(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> SerializeSeq for IndexedKeyValueSerializer<'a> {
    type Ok = ();
    type Error = EncodingError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.process(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> SerializeTupleStruct for IndexedKeyValueSerializer<'a> {
    type Ok = ();
    type Error = EncodingError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.process(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> SerializeTupleVariant for IndexedKeyValueSerializer<'a> {
    type Ok = ();
    type Error = EncodingError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.process(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

struct KeyedKeyValueSerializer<'a> {
    key: Option<String>,
    ser: KeyValueSerializer<'a>,
}

impl<'a> SerializeMap for KeyedKeyValueSerializer<'a> {
    type Ok = ();
    type Error = EncodingError;
    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        use serde_json::{to_value, Value};
        match to_value(key) {
            Ok(Value::String(key)) => {
                self.key = Some(key);
                Ok(())
            }
            _ => Err(EncodingError::KeyNotString),
        }
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        let key = self.key.take().expect("Key must present.");
        value.serialize(self.ser.child(key))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}
