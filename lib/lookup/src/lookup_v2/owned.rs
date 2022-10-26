use crate::lookup_v2::{
    parse_target_path, parse_value_path, BorrowedSegment, PathParseError, ValuePath,
};
use crate::PathPrefix;
use std::fmt::{Debug, Display, Formatter};
use vector_config::configurable_component;

/// A lookup path.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct OwnedValuePath {
    pub segments: Vec<OwnedSegment>,
}

impl OwnedValuePath {
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

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

    /// Create the possible fields that can be followed by this lookup.
    /// Because of coalesced paths there can be a number of different combinations.
    /// There is the potential for this function to create a vast number of different
    /// combinations if there are multiple coalesced segments in a path.
    ///
    /// The limit specifies the limit of the path depth we are interested in.
    /// Metrics is only interested in fields that are up to 3 levels deep (2 levels + 1 to check it
    /// terminates).
    ///
    /// eg, .tags.nork.noog will never be an accepted path so we don't need to spend the time
    /// collecting it.
    pub fn to_alternative_components(&self, limit: usize) -> Vec<Vec<&str>> {
        let mut components = vec![vec![]];
        for segment in self.segments.iter().take(limit) {
            match segment {
                OwnedSegment::Field(field) => {
                    for component in &mut components {
                        component.push(field.as_str());
                    }
                }

                OwnedSegment::Coalesce(fields) => {
                    components = components
                        .iter()
                        .flat_map(|path| {
                            fields.iter().map(move |field| {
                                let mut path = path.clone();
                                path.push(field.as_str());
                                path
                            })
                        })
                        .collect();
                }

                OwnedSegment::Index(_) => {
                    return Vec::new();
                }
            }
        }

        components
    }

    pub fn push(&mut self, segment: OwnedSegment) {
        self.segments.push(segment);
    }
}

/// An owned path that contains a target (pointing to either an Event or Metadata)
#[configurable_component]
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct OwnedTargetPath {
    pub prefix: PathPrefix,
    pub path: OwnedValuePath,
}

impl OwnedTargetPath {
    pub fn event_root() -> Self {
        Self::root(PathPrefix::Event)
    }
    pub fn metadata_root() -> Self {
        Self::root(PathPrefix::Metadata)
    }

    pub fn root(prefix: PathPrefix) -> Self {
        Self {
            prefix,
            path: OwnedValuePath::root(),
        }
    }

    pub fn event(path: OwnedValuePath) -> Self {
        Self {
            prefix: PathPrefix::Event,
            path,
        }
    }

    pub fn metadata(path: OwnedValuePath) -> Self {
        Self {
            prefix: PathPrefix::Metadata,
            path,
        }
    }

    pub fn can_start_with(&self, prefix: &Self) -> bool {
        if self.prefix != prefix.prefix {
            return false;
        }
        (&self.path).can_start_with(&prefix.path)
    }

    pub fn with_field_appended(&self, field: &str) -> Self {
        let mut new_path = self.path.clone();
        new_path.push_field(field);
        Self {
            prefix: self.prefix,
            path: new_path,
        }
    }

    pub fn with_index_appended(&self, index: isize) -> Self {
        let mut new_path = self.path.clone();
        new_path.push_index(index);
        Self {
            prefix: self.prefix,
            path: new_path,
        }
    }
}

impl Display for OwnedTargetPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self.to_owned()))
    }
}

impl Debug for OwnedTargetPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl From<OwnedTargetPath> for String {
    fn from(target_path: OwnedTargetPath) -> Self {
        match target_path.prefix {
            PathPrefix::Event => format!(".{}", target_path.path),
            PathPrefix::Metadata => format!("%{}", target_path.path),
        }
    }
}

impl Display for OwnedValuePath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self.clone()))
    }
}

impl TryFrom<String> for OwnedValuePath {
    type Error = PathParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        parse_value_path(&src).map_err(|_| PathParseError::InvalidPathSyntax {
            path: src.to_owned(),
        })
    }
}

impl TryFrom<String> for OwnedTargetPath {
    type Error = PathParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        parse_target_path(&src).map_err(|_| PathParseError::InvalidPathSyntax {
            path: src.to_owned(),
        })
    }
}

impl From<OwnedValuePath> for String {
    fn from(owned: OwnedValuePath) -> Self {
        let mut coalesce_i = 0;
        owned
            .segments
            .iter()
            .enumerate()
            .map(|(i, segment)| match segment {
                OwnedSegment::Field(field) => {
                    serialize_field(field.as_ref(), (i != 0).then_some("."))
                }
                OwnedSegment::Index(index) => format!("[{}]", index),
                OwnedSegment::Coalesce(fields) => {
                    let mut output = String::new();
                    let (last, fields) = fields.split_last().expect("coalesce must not be empty");
                    for field in fields {
                        let field_output = serialize_field(
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
                        output.push_str(&field_output);
                    }
                    output += &serialize_field(last.as_ref(), (coalesce_i != 0).then_some("|"));
                    output += ")";
                    output
                }
            })
            .collect::<Vec<_>>()
            .join("")
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

impl From<Vec<OwnedSegment>> for OwnedValuePath {
    fn from(segments: Vec<OwnedSegment>) -> Self {
        Self { segments }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum OwnedSegment {
    Field(String),
    Index(isize),
    Coalesce(Vec<String>),
}

impl OwnedSegment {
    pub fn field(value: &str) -> OwnedSegment {
        OwnedSegment::Field(value.to_string())
    }
    pub fn index(value: isize) -> OwnedSegment {
        OwnedSegment::Index(value)
    }

    pub fn coalesce(fields: Vec<String>) -> OwnedSegment {
        OwnedSegment::Coalesce(fields)
    }

    pub fn is_field(&self) -> bool {
        matches!(self, OwnedSegment::Field(_))
    }
    pub fn is_index(&self) -> bool {
        matches!(self, OwnedSegment::Index(_))
    }

    pub fn can_start_with(&self, prefix: &OwnedSegment) -> bool {
        match (self, prefix) {
            (OwnedSegment::Index(a), OwnedSegment::Index(b)) => a == b,
            (OwnedSegment::Index(_), _) | (_, OwnedSegment::Index(_)) => false,
            (OwnedSegment::Field(a), OwnedSegment::Field(b)) => a == b,
            (OwnedSegment::Field(field), OwnedSegment::Coalesce(fields))
            | (OwnedSegment::Coalesce(fields), OwnedSegment::Field(field)) => {
                fields.contains(field)
            }
            (OwnedSegment::Coalesce(a), OwnedSegment::Coalesce(b)) => {
                a.iter().any(|a_field| b.contains(a_field))
            }
        }
    }
}

impl From<Vec<&'static str>> for OwnedSegment {
    fn from(fields: Vec<&'static str>) -> Self {
        fields
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .into()
    }
}

impl From<Vec<String>> for OwnedSegment {
    fn from(fields: Vec<String>) -> Self {
        OwnedSegment::Coalesce(fields)
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

impl<'a> ValuePath<'a> for &'a Vec<OwnedSegment> {
    type Iter = OwnedSegmentSliceIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        OwnedSegmentSliceIter {
            segments: self.as_slice(),
            index: 0,
            coalesce_i: 0,
        }
    }
}

impl<'a> ValuePath<'a> for &'a [OwnedSegment] {
    type Iter = OwnedSegmentSliceIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        OwnedSegmentSliceIter {
            segments: self,
            index: 0,
            coalesce_i: 0,
        }
    }
}

impl<'a> ValuePath<'a> for &'a OwnedValuePath {
    type Iter = OwnedSegmentSliceIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        (&self.segments).segment_iter()
    }
}

#[derive(Clone)]
pub struct OwnedSegmentSliceIter<'a> {
    segments: &'a [OwnedSegment],
    index: usize,
    coalesce_i: usize,
}

impl<'a> Iterator for OwnedSegmentSliceIter<'a> {
    type Item = BorrowedSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let output = self.segments.get(self.index).map(|segment| match segment {
            OwnedSegment::Field(field) => BorrowedSegment::Field(field.as_str().into()),
            OwnedSegment::Index(i) => BorrowedSegment::Index(*i),
            OwnedSegment::Coalesce(fields) => {
                let coalesce_segment;
                if self.coalesce_i == fields.len() - 1 {
                    coalesce_segment =
                        BorrowedSegment::CoalesceEnd(fields[self.coalesce_i].as_str().into());
                    self.coalesce_i = 0;
                } else {
                    coalesce_segment =
                        BorrowedSegment::CoalesceField(fields[self.coalesce_i].as_str().into());
                    self.coalesce_i += 1;
                }
                coalesce_segment
            }
        });
        if self.coalesce_i == 0 {
            self.index += 1;
        }
        output
    }
}

#[cfg(test)]
mod test {
    use crate::lookup_v2::parse_value_path;

    #[test]
    fn owned_path_serialize() {
        let test_cases = [
            (".", Some("")),
            ("", None),
            ("]", None),
            ("]foo", None),
            ("..", None),
            ("...", None),
            ("f", Some("f")),
            ("foo", Some("foo")),
            (
                r#"ec2.metadata."availability-zone""#,
                Some(r#"ec2.metadata."availability-zone""#),
            ),
            ("@timestamp", Some("@timestamp")),
            ("foo[", None),
            ("foo$", None),
            (r#""$peci@l chars""#, Some(r#""$peci@l chars""#)),
            ("foo.foo bar", None),
            (r#"foo."foo bar".bar"#, Some(r#"foo."foo bar".bar"#)),
            ("[1]", Some("[1]")),
            ("[42]", Some("[42]")),
            ("foo.[42]", None),
            ("[42].foo", Some("[42].foo")),
            ("[-1]", Some("[-1]")),
            ("[-42]", Some("[-42]")),
            ("[-42].foo", Some("[-42].foo")),
            ("[-42]foo", Some("[-42].foo")),
            (r#""[42]. {}-_""#, Some(r#""[42]. {}-_""#)),
            (r#""a\"a""#, Some(r#""a\"a""#)),
            (r#"foo."a\"a"."b\\b".bar"#, Some(r#"foo."a\"a"."b\\b".bar"#)),
            ("<invalid>", None),
            (r#""🤖""#, Some(r#""🤖""#)),
            (".(a|b)", Some("(a|b)")),
            (".(a|b|c)", Some("(a|b|c)")),
            ("foo.(a|b|c)", Some("foo.(a|b|c)")),
            ("[0].(a|b|c)", Some("[0].(a|b|c)")),
            (".(a|b|c).foo", Some("(a|b|c).foo")),
            (".( a | b | c ).foo", Some("(a|b|c).foo")),
        ];

        for (path, expected) in test_cases {
            let path = parse_value_path(path).map(String::from).ok();

            assert_eq!(path, expected.map(|x| x.to_owned()));
        }
    }
}
