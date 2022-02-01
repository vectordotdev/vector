// use std::collections::BTreeMap;

// use lookup::{FieldBuf, LookupBuf, SegmentBuf};

// use super::Value;

//TODO: Move tests to Value

// #[cfg(test)]
// mod tests {
//     use crate::value;
//
//     #[test]
//     fn test_object() {
//         let path = parser::parse_path(".foo.bar.baz").unwrap();
//         let value = value!(12);
//
//         let object = value!({ "foo": { "bar": { "baz": 12 } } });
//
//         assert_eq!(value.at_path(&path), object);
//     }
//
//     #[test]
//     fn test_root() {
//         let path = parser::parse_path(".").unwrap();
//         let value = value!(12);
//
//         let object = value!(12);
//
//         assert_eq!(value.at_path(&path), object);
//     }
//
//     #[test]
//     fn test_array() {
//         let path = parser::parse_path(".[2]").unwrap();
//         let value = value!(12);
//
//         let object = value!([null, null, 12]);
//
//         assert_eq!(value.at_path(&path), object);
//     }
//
//     #[test]
//     fn test_complex() {
//         let path = parser::parse_path(".[2].foo.(bar | baz )[1]").unwrap();
//         let value = value!({ "bar": [12] });
//
//         let object = value!([null, null, { "foo": { "baz": [null, { "bar": [12] }] } } ]);
//
//         assert_eq!(value.at_path(&path), object);
//     }
// }
