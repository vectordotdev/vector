use super::Error as E;
use crate::{CompilerState, Expression, Object, ResolveKind, Result, State, Value};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("missing path: {0}")]
    Missing(String),

    #[error("unable to resolve path: {0}")]
    Resolve(String),
}

#[derive(Debug, Clone)]
pub struct Path {
    // TODO: Switch to String once Event API is cleaned up.
    segments: Vec<Vec<String>>,
}

impl<T: AsRef<str>> From<T> for Path {
    fn from(v: T) -> Self {
        Self {
            segments: vec![vec![v.as_ref().to_owned()]],
        }
    }
}

impl Path {
    pub(crate) fn new(segments: Vec<Vec<String>>) -> Self {
        Self { segments }
    }
}

impl Expression for Path {
    fn execute(&self, _: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        object
            .find(&self.segments)
            .map_err(|e| E::from(Error::Resolve(e)))?
            .ok_or_else(|| E::from(Error::Missing(segments_to_path(&self.segments))).into())
            .map(Some)
    }

    /// A path resolves to `Any` by default, but the script might assign
    /// specific values to paths during its execution, which increases our exact
    /// understanding of the value kind the path contains.
    fn resolves_to(&self, state: &CompilerState) -> ResolveKind {
        state
            .path_query_kind(&segments_to_path(&self.segments))
            .cloned()
            .unwrap_or(ResolveKind::Any)
    }
}

pub(crate) fn segments_to_path(segments: &[Vec<String>]) -> String {
    segments
        .iter()
        .map(|c| {
            c.iter()
                .map(|p| p.replace(".", "\\."))
                .collect::<Vec<_>>()
                .join(".")
        })
        .collect::<Vec<_>>()
        .join(".")
}
