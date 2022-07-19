use crate::lookup_v2::{parse_path, BorrowedSegment, Path};
use vector_config::configurable_component;

/// A lookup path.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[serde(from = "String", into = "String")]
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

impl From<String> for OwnedPath {
    fn from(raw_path: String) -> Self {
        parse_path(raw_path.as_str())
    }
}

impl From<OwnedPath> for String {
    fn from(owned: OwnedPath) -> Self {
        if owned.segments.is_empty() {
            String::from("<invalid>")
        } else {
            let mut coalesce_i = 0;

            owned
                .segments
                .iter()
                .enumerate()
                .map(|(i, segment)| match segment {
                    OwnedSegment::Field(field) => {
                        serialize_field(field.as_ref(), (i != 0).then(|| "."))
                    }

                    OwnedSegment::CoalesceField(field) => {
                        let output = serialize_field(
                            field.as_ref(),
                            Some(if coalesce_i == 0 {
                                if i == 0 {
                                    "("
                                } else {
                                    ".("
                                }
                            } else {
                                "|"
                            }),
                        );
                        coalesce_i += 1;
                        output
                    }
                    OwnedSegment::Index(index) => format!("[{}]", index),
                    OwnedSegment::Invalid => {
                        (if i == 0 { "<invalid>" } else { ".<invalid>" }).to_owned()
                    }
                    OwnedSegment::CoalesceEnd(field) => {
                        format!(
                            "{})",
                            serialize_field(field.as_ref(), (coalesce_i != 0).then(|| "|"))
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        }
    }
}

fn serialize_field(field: &str, separator: Option<&str>) -> String {
    // These characters should match the ones from the parser, implemented in `JitLookup`
    let needs_quotes = field
        .chars()
        .any(|c| !matches!(c, 'A'..='Z' | 'a'..='z' | '_' | '0'..='9' | '@'));

    // Allocate enough to fit the field, a `.` and two `"` characters. This
    // should suffice for the majority of cases when no escape sequence is used.
    let separator_len = separator.map(|x| x.len()).unwrap_or(0);
    let mut string = String::with_capacity(field.as_bytes().len() + 2 + separator_len);
    if let Some(separator) = separator {
        string.push_str(separator);
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnedSegment {
    Field(String),
    Index(isize),
    CoalesceField(String),
    CoalesceEnd(String),
    Invalid,
}

impl OwnedSegment {
    pub fn field(value: &str) -> OwnedSegment {
        OwnedSegment::Field(value.to_string())
    }
    pub fn index(value: isize) -> OwnedSegment {
        OwnedSegment::Index(value)
    }

    pub fn coalesce_field(field: impl Into<String>) -> OwnedSegment {
        OwnedSegment::CoalesceField(field.into())
    }

    pub fn coalesce_end(field: impl Into<String>) -> OwnedSegment {
        OwnedSegment::CoalesceEnd(field.into())
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
            BorrowedSegment::Field(value) => OwnedSegment::Field(value.to_string()),
            BorrowedSegment::Index(value) => OwnedSegment::Index(value),
            BorrowedSegment::Invalid => OwnedSegment::Invalid,
            BorrowedSegment::CoalesceField(field) => OwnedSegment::CoalesceField(field.to_string()),
            BorrowedSegment::CoalesceEnd(field) => OwnedSegment::CoalesceEnd(field.to_string()),
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
            (".(a|b)", "(a|b)"),
            (".(a|b|c)", "(a|b|c)"),
            ("foo.(a|b|c)", "foo.(a|b|c)"),
            ("[0].(a|b|c)", "[0].(a|b|c)"),
            (".(a|b|c).foo", "(a|b|c).foo"),
            (".( a | b | c ).foo", "(a|b|c).foo"),
        ];

        for (path, expected) in test_cases {
            let path = parse_path(path);
            let path = serde_json::to_string(&path).unwrap();
            let path = serde_json::from_str::<serde_json::Value>(&path).unwrap();
            assert_eq!(path, expected);
        }
    }
}
