/// A `field` type that can be used in `Collection<Field>`
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Field(lookup::FieldBuf);

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Field {
    /// Get a `str` representation of the field.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
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
