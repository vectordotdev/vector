use crate::kind::collection::{CollectionKey, CollectionRemove};
use crate::kind::Collection;
use lookup::lookup_v2::OwnedSegment;

/// A `field` type that can be used in `Collection<Field>`
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Field(lookup::FieldBuf);

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl CollectionKey for Field {
    fn to_segment(&self) -> OwnedSegment {
        OwnedSegment::Field(self.0.name.clone())
    }
}

impl Field {
    /// Get a `str` representation of the field.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl CollectionRemove for Collection<Field> {
    type Key = Field;

    fn remove_known(&mut self, key: &Field) {
        self.known.remove(key);
    }
}

impl std::ops::Deref for Field {
    type Target = lookup::FieldBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for Field {
    fn from(field: &str) -> Self {
        Self(field.into())
    }
}

impl From<String> for Field {
    fn from(field: String) -> Self {
        Self(field.into())
    }
}

impl From<lookup::FieldBuf> for Field {
    fn from(field: lookup::FieldBuf) -> Self {
        Self(field)
    }
}

impl From<Field> for lookup::FieldBuf {
    fn from(field: Field) -> Self {
        field.0
    }
}

impl From<lookup::Field<'_>> for Field {
    fn from(field: lookup::Field<'_>) -> Self {
        (&field).into()
    }
}

impl From<&lookup::Field<'_>> for Field {
    fn from(field: &lookup::Field<'_>) -> Self {
        Self(field.as_field_buf())
    }
}

impl<'a> From<&'a Field> for lookup::Field<'a> {
    fn from(field: &'a Field) -> Self {
        (&field.0).into()
    }
}
