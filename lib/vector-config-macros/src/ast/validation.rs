use serde_derive_internals::Ctxt;
use syn::NestedMeta;

/// Well-known validator formats from the JSONSchema validation specification itself.
///
/// Not all defined formats are present here.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Format {
    Email,
    Phone,
    Url,
}

/// A validation definition.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ValidationDef {
    KnownFormat(Format),
    Length { minimum: u32, maximum: u32 },
    Range { minimum: f64, maximum: f64 },
    Contains(String),
    Regex(String),
}

impl ValidationDef {
    /// Attempts to parse validation definitions from a list of attribute meta items.
    ///
    /// If any of the meta items is unknown, or is invalid, the error context will be updated with a
    /// descriptive error that describes the full scope of the error, and `Err(())` will be returned.
    ///
    /// Otherwise, a list of all valid validation definitions, if any, will be returned.
    pub fn parse_defs<'a, M>(_context: &Ctxt, _meta_items: M) -> Result<Vec<ValidationDef>, ()>
    where
        M: Iterator<Item = &'a NestedMeta> + 'a,
    {
        // TODO: Copy code from `FieldAttributes`, basically, for parsing.
        todo!()
    }
}
