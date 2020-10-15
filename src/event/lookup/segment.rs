use crate::{event::PathComponent, mapping::parser::Rule};
use pest::iterators::Pair;
use std::fmt::{Display, Formatter};

/// Segments are chunks of a lookup. They represent either a field or an index.
/// A sequence of Segments can become a lookup.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum Segment {
    Field(String),
    QuotedField(String),
    Index(usize),
}

impl Segment {
    pub const fn field(v: String) -> Segment {
        Segment::Field(v)
    }

    pub fn is_field(&self) -> bool {
        matches!(self, Segment::Field(_))
    }

    pub const fn quoted_field(v: String) -> Segment {
        Segment::QuotedField(v)
    }

    pub fn is_quoted_field(&self) -> bool {
        matches!(self, Segment::QuotedField(_))
    }

    pub const fn index(v: usize) -> Segment {
        Segment::Index(v)
    }

    pub fn is_index(&self) -> bool {
        matches!(self, Segment::Index(_))
    }

    #[tracing::instrument(skip(path))]
    pub(crate) fn from_lookup(path: Pair<'_, Rule>) -> crate::Result<Vec<Segment>> {
        let rule = path.as_rule();
        tracing::trace!(path = path.as_str(), ?rule, action = %"enter");
        let mut segments = Vec::default();
        for segment in path.into_inner() {
            for inner_segment in segment.into_inner() {
                match inner_segment.as_rule() {
                    Rule::path_field => {
                        tracing::trace!(segment = inner_segment.as_str(), rule = ?inner_segment.as_rule(), action = %"push");
                        segments.push(Segment::field(inner_segment.as_str().to_owned()));
                    }
                    Rule::path_field_quoted => {
                        segments.push(Segment::from_quoted_path_segment(inner_segment)?)
                    }
                    Rule::path_index => segments.push(Segment::from_path_index(inner_segment)?),
                    _ => {
                        return Err(format!(
                            "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                            inner_segment.as_rule(),
                            [Rule::path_field, Rule::path_field_quoted, Rule::path_index]
                        )
                        .into())
                    }
                }
            }
        }
        tracing::trace!(segment = ?segments, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_path_index(segment: Pair<'_, Rule>) -> crate::Result<Segment> {
        let full_segment = segment.as_str();
        tracing::trace!(segment = full_segment, rule = ?segment.as_rule(), action = %"enter");
        let segment = segment.into_inner().next().expect(
            "Did not get pair inside path_index segment. This is an invariant. Please report it.",
        );
        let retval = match segment.as_rule() {
            Rule::inner_path_index => {
                let index = segment.as_str().parse()?;
                tracing::trace!(segment = index, rule = ?segment.as_rule(), action = %"push");
                Ok(Segment::index(index))
            }
            _ => Err(format!(
                "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                segment.as_rule(),
                [Rule::inner_path_index,]
            )
            .into()),
        };
        tracing::trace!(segment = full_segment, rule = ?segment.as_rule(), action = %"exit");
        retval
    }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_quoted_path_segment(segment: Pair<'_, Rule>) -> crate::Result<Segment> {
        let full_segment = segment.as_str();
        tracing::trace!(segment = full_segment, rule = ?segment.as_rule(), action = %"enter");
        let segment = segment.into_inner().next()
            .expect("Did not get pair inside quoted_path_segment segment. This is an invariant. Please report it.");
        let retval = match segment.as_rule() {
            Rule::inner_quoted_string => {
                tracing::trace!(segment = segment.as_str(), rule = ?segment.as_rule(), action = %"push");
                Ok(Segment::quoted_field(segment.as_str().to_owned()))
            }
            _ => {
                return Err(format!(
                    "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                    segment.as_rule(),
                    [Rule::inner_quoted_string,]
                )
                .into())
            }
        };
        tracing::trace!(segment = full_segment, rule = ?segment.as_rule(), action = %"exit");
        retval
    }
}

impl Display for Segment {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Segment::Index(i) => write!(formatter, "{}", i),
            Segment::Field(f) | Segment::QuotedField(f) => write!(formatter, "{}", f),
        }
    }
}

impl From<String> for Segment {
    fn from(s: String) -> Self {
        Self::Field(s)
    }
}

impl From<usize> for Segment {
    fn from(u: usize) -> Self {
        Self::index(u)
    }
}

impl From<Segment> for PathComponent {
    fn from(segment: Segment) -> Self {
        match segment {
            Segment::Field(f) | Segment::QuotedField(f) => PathComponent::Key(f),
            Segment::Index(i) => PathComponent::Index(i),
        }
    }
}
