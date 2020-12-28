use super::{query_value::QueryValue, Function};
use crate::event::util::log::PathIter;
use crate::{
    event::{util::log::get_value, Event, Lookup},
    mapping::Result,
};

#[derive(Debug)]
pub(in crate::mapping) struct Path {
    // TODO: Switch to String once Event API is cleaned up.
    path: Vec<Vec<String>>,
}

impl From<&str> for Path {
    fn from(target: &str) -> Self {
        Self {
            path: vec![vec![target.into()]],
        }
    }
}

impl From<Vec<Vec<String>>> for Path {
    fn from(path: Vec<Vec<String>>) -> Self {
        Self {
            path: path
                .iter()
                .map(|c| c.iter().map(|p| p.replace(".", "\\.")).collect())
                .collect(),
        }
    }
}

impl From<Vec<Vec<&str>>> for Path {
    fn from(path: Vec<Vec<&str>>) -> Self {
        Self {
            // TODO: Switch to String once Event API is cleaned up.
            path: path
                .iter()
                .map(|c| c.iter().map(|p| p.replace(".", "\\.")).collect())
                .collect(),
        }
    }
}

impl Function for Path {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        // Event.as_log returns a LogEvent struct rather than a naked
        // IndexMap<_, Value>, which means specifically for the first item in
        // the path we need to manually call .get.
        //
        // If we could simply pull either an IndexMap or Value out of a LogEvent
        // then we wouldn't need this duplicate code as we'd jump straight into
        // the path walker.
        let mut value = self.path[0]
            .iter()
            .find_map(|p| ctx.as_log().get(Lookup::from(p.as_str())))
            .ok_or_else(|| format!("path .{} not found in event", self.path[0].first().unwrap()))?;

        // Walk remaining (if any) path segments. Our parse is already capable
        // of extracting individual path tokens from user input. For example,
        // the path `.foo."bar.baz"[0]` could potentially be pulled out into
        // the tokens `foo`, `bar.baz`, `0`. However, the Value API doesn't
        // allow for traversing that way and we'd therefore need to implement
        // our own walker.
        //
        // For now we're broken as we're using an API that assumes unescaped
        // dots are path delimiters. We either need to escape dots within the
        // path and take the hit of bridging one escaping mechanism with another
        // or when we refactor the value API we add options for providing
        // unescaped tokens.
        for (i, segments) in self.path.iter().enumerate().skip(1) {
            value = segments
                .iter()
                .find_map(|p| get_value(value, PathIter::new(p)))
                .ok_or_else(|| {
                    format!(
                        "path {} not found in event",
                        self.path
                            .iter()
                            .take(i + 1)
                            .fold("".to_string(), |acc, p| format!(
                                "{}.{}",
                                acc,
                                p.first().unwrap()
                            ),)
                    )
                })?;
        }

        Ok(value.clone().into())
    }
}
