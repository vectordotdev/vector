use std::{
    str,
};
use crate::mapping::parser::{Rule};
use pest::{iterators::Pair};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub enum Segment<'a> {
    Field(&'a str),
    Index(usize),
}

impl<'a> Segment<'a> {
    pub const fn field(v: &'a str) -> Segment<'a> { Segment::Field(v) }
    pub const fn index(v: usize) -> Segment<'a> { Segment::Index(v) }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_lookup(segment: Pair<'_, Rule>) -> crate::Result<Vec<Segment>> {
        let rule = segment.as_rule();
        tracing::trace!(segment = segment.as_str(), ?rule, action = %"enter");
        let mut segments = Vec::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                Rule::path_segment => segments.append(&mut Segment::from_path_segment(inner_segment)?),
                Rule::quoted_path_segment => {
                    segments.push(Segment::from_quoted_path_segment(inner_segment)?)
                },
                _ => return Err(format!("Got invalid lookup rule. Got: {:?}. Want: {:?}", inner_segment.as_rule(), [
                    Rule::path_segment,
                    Rule::quoted_path_segment
                ]).into()),
            }
        }
        tracing::trace!(segment = ?segments, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_path_segment(segment: Pair<'_, Rule>) -> crate::Result<Vec<Segment>> {
        let rule = segment.as_rule();
        tracing::trace!(segment = segment.as_str(), ?rule, action = %"enter");
        let mut segments = Vec::default();
        for inner_segment in segment.into_inner() {
            match inner_segment.as_rule() {
                Rule::path_field_name => {
                    tracing::trace!(segment = inner_segment.as_str(), rule = ?inner_segment.as_rule(), action = %"push");
                    segments.push(Segment::field(inner_segment.as_str()))
                },
                Rule::path_index => segments.push(Segment::from_path_index(inner_segment)?),
                _ => return Err(format!("Got invalid lookup rule. Got: {:?}. Want: {:?}", inner_segment.as_rule(), [
                    Rule::path_field_name,
                    Rule::path_index
                ]).into()),
            }
        }
        tracing::trace!(segment = ?segments, ?rule, action = %"exit");
        Ok(segments)
    }

    #[tracing::instrument(skip(segment))]
    pub(crate) fn from_path_index(segment: Pair<'_, Rule>) -> crate::Result<Segment> {
        let full_segment = segment.as_str();
        tracing::trace!(segment = full_segment, rule = ?segment.as_rule(), action = %"enter");
        let segment = segment.into_inner().next()
            .expect("Did not get pair inside path_index segment. This is an invariant. Please report it.");
        let retval = match segment.as_rule() {
            Rule::inner_path_index => {
                let index = segment.as_str().parse()?;
                tracing::trace!(segment = index, rule = ?segment.as_rule(), action = %"push");
                Ok(Segment::index(index))
            },
            _ => Err(format!("Got invalid lookup rule. Got: {:?}. Want: {:?}", segment.as_rule(), [
                Rule::inner_path_index,
            ]).into()),
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
                Ok(Segment::field(segment.as_str()))
            },
            _ => return Err(format!("Got invalid lookup rule. Got: {:?}. Want: {:?}", segment.as_rule(), [
                Rule::inner_quoted_string,
            ]).into()),
        };
        tracing::trace!(segment = full_segment, rule = ?segment.as_rule(), action = %"exit");
        retval
    }
}
