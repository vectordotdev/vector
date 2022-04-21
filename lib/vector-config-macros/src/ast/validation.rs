use darling::FromMeta;

/// Well-known validator formats from the JSONSchema validation specification itself.
///
/// Not all defined formats are present here.
#[derive(Debug, FromMeta)]
pub enum Format {
    Email,
    Phone,
    Uri,
}

/// A validation definition.
#[derive(Debug, FromMeta)]
pub enum Validation {
    #[darling(rename = "format")]
    KnownFormat(Format),
    Length {
        #[darling(rename = "min")]
        minimum: u32,
        #[darling(rename = "max")]
        maximum: u32
    },
    Range {
        #[darling(rename = "min")]
        minimum: f64,
        #[darling(rename = "max")]
        maximum: f64
    },
    Contains(String),
    Regex(String),
}
