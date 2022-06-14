use crate::lookup_v2::{parse_path, BorrowedSegment, Path};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnedPath {
    pub segments: Vec<OwnedSegment>,
}

impl OwnedPath {
    pub fn root() -> Self {
        vec![].into()
    }

    pub fn push_field(&mut self, field: &str) {
        self.segments.push(OwnedSegment::field(field));
    }

    pub fn with_field_appended(&self, field: &str) -> Self {
        let mut new_path = self.clone();
        new_path.push_field(field);
        new_path
    }

    pub fn push_index(&mut self, index: isize) {
        self.segments.push(OwnedSegment::index(index));
    }

    pub fn with_index_appended(&self, index: isize) -> Self {
        let mut new_path = self.clone();
        new_path.push_index(index);
        new_path
    }

    pub fn single_field(field: &str) -> Self {
        vec![OwnedSegment::field(field)].into()
    }
}

impl<'de> Deserialize<'de> for OwnedPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path: String = Deserialize::deserialize(deserializer)?;
        Ok(parse_path(&path))
    }
}

impl Serialize for OwnedPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.segments.is_empty() {
            serializer.serialize_str("<invalid>")
        } else {
            let path = self
                .segments
                .iter()
                .enumerate()
                .map(|(i, segment)| match segment {
                    OwnedSegment::Field(field) => {
                        let needs_quotes = field
                            .chars()
                            .any(|c| !matches!(c, 'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@'));
                        // Allocate enough to fit the field, a `.` and two `"` characters. This
                        // should suffice for the majority of cases when no escape sequence is used.
                        let mut string = String::with_capacity(field.as_bytes().len() + 3);
                        if i != 0 {
                            string.push('.');
                        }
                        if needs_quotes {
                            string.push('"');
                            for c in field.chars() {
                                if matches!(c, '"' | '\\') {
                                    string.push('\\');
                                }
                                string.push(c);
                            }
                            string.push('"');
                            string
                        } else {
                            string.push_str(field);
                            string
                        }
                    }
                    OwnedSegment::Index(index) => format!("[{}]", index),
                    OwnedSegment::Invalid => {
                        (if i == 0 { "<invalid>" } else { ".<invalid>" }).to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("");
            serializer.serialize_str(&path)
        }
    }
}

impl From<Vec<OwnedSegment>> for OwnedPath {
    fn from(segments: Vec<OwnedSegment>) -> Self {
        Self { segments }
    }
}

impl<const N: usize> From<[OwnedSegment; N]> for OwnedPath {
    fn from(segments: [OwnedSegment; N]) -> Self {
        OwnedPath::from(Vec::from(segments))
    }
}

impl<'a, const N: usize> From<[BorrowedSegment<'a>; N]> for OwnedPath {
    fn from(segments: [BorrowedSegment<'a>; N]) -> Self {
        OwnedPath::from(segments.map(OwnedSegment::from))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum OwnedSegment {
    Field(String),
    Index(isize),
    Invalid,
}

impl OwnedSegment {
    pub fn field(value: &str) -> OwnedSegment {
        OwnedSegment::Field(value.into())
    }
    pub fn index(value: isize) -> OwnedSegment {
        OwnedSegment::Index(value)
    }
    pub fn is_field(&self) -> bool {
        matches!(self, OwnedSegment::Field(_))
    }
    pub fn is_index(&self) -> bool {
        matches!(self, OwnedSegment::Index(_))
    }
    pub fn is_invalid(&self) -> bool {
        matches!(self, OwnedSegment::Invalid)
    }
}

impl<'a> From<BorrowedSegment<'a>> for OwnedSegment {
    fn from(x: BorrowedSegment<'a>) -> Self {
        match x {
            BorrowedSegment::Field(value) => OwnedSegment::Field((*value).to_owned()),
            BorrowedSegment::Index(value) => OwnedSegment::Index(value),
            BorrowedSegment::Invalid => OwnedSegment::Invalid,
        }
    }
}

impl<'a> From<&'a str> for OwnedSegment {
    fn from(field: &'a str) -> Self {
        OwnedSegment::field(field)
    }
}

impl<'a> From<&'a String> for OwnedSegment {
    fn from(field: &'a String) -> Self {
        OwnedSegment::field(field.as_str())
    }
}

impl From<isize> for OwnedSegment {
    fn from(index: isize) -> Self {
        OwnedSegment::index(index)
    }
}

impl<'a> Path<'a> for &'a Vec<OwnedSegment> {
    type Iter = OwnedSegmentSliceIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        OwnedSegmentSliceIter {
            segments: self.as_slice(),
            index: 0,
        }
    }
}

impl<'a> Path<'a> for &'a OwnedPath {
    type Iter = OwnedSegmentSliceIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        (&self.segments).segment_iter()
    }
}

pub struct OwnedSegmentSliceIter<'a> {
    segments: &'a [OwnedSegment],
    index: usize,
}

impl<'a> Iterator for OwnedSegmentSliceIter<'a> {
    type Item = BorrowedSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let output = self.segments.get(self.index).map(|x| x.into());
        self.index += 1;
        output
    }
}

#[cfg(test)]
mod test {
    use crate::lookup_v2::parse_path;

    #[test]
    fn owned_path_serialize() {
        let test_cases = [
            ("", "<invalid>"),
            ("]", "<invalid>"),
            ("]foo", "<invalid>"),
            ("..", "<invalid>"),
            ("...", "<invalid>"),
            ("f", "f"),
            ("foo", "foo"),
            (
                r#"ec2.metadata."availability-zone""#,
                r#"ec2.metadata."availability-zone""#,
            ),
            ("@timestamp", "@timestamp"),
            ("foo[", "foo.<invalid>"),
            ("foo$", "<invalid>"),
            (r#""$peci@l chars""#, r#""$peci@l chars""#),
            ("foo.foo bar", "foo.<invalid>"),
            (r#"foo."foo bar".bar"#, r#"foo."foo bar".bar"#),
            ("[1]", "[1]"),
            ("[42]", "[42]"),
            ("foo.[42]", "foo.<invalid>"),
            ("[42].foo", "[42].foo"),
            ("[-1]", "[-1]"),
            ("[-42]", "[-42]"),
            ("[-42].foo", "[-42].foo"),
            ("[-42]foo", "[-42].foo"),
            (r#""[42]. {}-_""#, r#""[42]. {}-_""#),
            (r#""a\"a""#, r#""a\"a""#),
            (r#"foo."a\"a"."b\\b".bar"#, r#"foo."a\"a"."b\\b".bar"#),
            ("<invalid>", "<invalid>"),
            (r#""🤖""#, r#""🤖""#),
        ];

        for (path, expected) in test_cases {
            let path = parse_path(path);
            let path = serde_json::to_string(&path).unwrap();
            let path = serde_json::from_str::<serde_json::Value>(&path).unwrap();
            assert_eq!(path, expected);
        }
    }
}
