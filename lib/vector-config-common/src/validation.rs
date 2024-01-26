use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{Expr, Lit, Meta};

use crate::{
    configurable_package_name_hack,
    num::{ERR_NUMERIC_OUT_OF_RANGE, NUMERIC_ENFORCED_LOWER_BOUND, NUMERIC_ENFORCED_UPPER_BOUND},
    schema::{InstanceType, SchemaObject},
};

/// Well-known validator formats as described in the [JSON Schema Validation specification][jsvs].
///
/// Not all defined formats are present here.
///
/// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02
#[derive(Clone, Debug, FromMeta)]
pub enum Format {
    /// A date.
    ///
    /// Conforms to the `full-date` production as outlined in [RFC 3339, section 5.6][rfc3339], and specified in the
    /// [JSON Schema Validation specification, section 7.3.1][jsvs].
    ///
    /// [rfc3339]: https://datatracker.ietf.org/doc/html/rfc3339#section-5.6
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.1
    Date,

    /// A time.
    ///
    /// Conforms to the `full-time` production as outlined in [RFC 3339, section 5.6][rfc3339], and specified in the
    /// [JSON Schema Validation specification, section 7.3.1][jsvs].
    ///
    /// [rfc3339]: https://datatracker.ietf.org/doc/html/rfc3339#section-5.6
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.1
    Time,

    /// A datetime.
    ///
    /// Conforms to the `date-time` production as outlined in [RFC 3339, section 5.6][rfc3339], and specified in the
    /// [JSON Schema Validation specification, section 7.3.1][jsvs].
    ///
    /// [rfc3339]: https://datatracker.ietf.org/doc/html/rfc3339#section-5.6
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.1
    #[darling(rename = "date-time")]
    DateTime,

    /// A duration.
    ///
    /// Conforms to the `duration` production as outlined in [RFC 3339, appendix A][rfc3339], and specified in the
    /// [JSON Schema Validation specification, section 7.3.1][jsvs].
    ///
    /// [rfc3339]: https://datatracker.ietf.org/doc/html/rfc3339#appendix-A
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.1
    Duration,

    /// An email address.
    ///
    /// Conforms to the `addr-spec` production as outlined in [RFC 5322, section 3.4.1][rfc5322], and specified in the
    /// [JSON Schema Validation specification, section 7.3.2][jsvs].
    ///
    /// [rfc5322]: https://datatracker.ietf.org/doc/html/rfc5322#section-3.4.1
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.2
    Email,

    /// An Internet hostname.
    ///
    /// Conforms to the `hname` production as outlined in [RFC 952, section "GRAMMATICAL HOST TABLE SPECIFICATION"][rfc952],
    /// and specified in the [JSON Schema Validation specification, section 7.3.3][jsvs].
    ///
    /// [rfc952]: https://datatracker.ietf.org/doc/html/rfc952
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.3
    Hostname,

    /// A uniform resource identifier (URI).
    ///
    /// Conforms to the `URI` production as outlined in [RFC 3986, appendix A][rfc3986], and specified in the [JSON
    /// Schema Validation specification, section 7.3.5][jsvs].
    ///
    /// [rfc3986]: https://datatracker.ietf.org/doc/html/rfc3986#appendix-A
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.5
    Uri,

    /// An IPv4 address.
    ///
    /// Conforms to the `dotted-quad` production as outlined in [RFC 2673, section 3.2][rfc2673], and specified in the
    /// [JSON Schema Validation specification, section 7.3.4][jsvs].
    ///
    /// [rfc2673]: https://datatracker.ietf.org/doc/html/rfc2673#section-3.2
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.4
    #[darling(rename = "ipv4")]
    IPv4,

    /// An IPv6 address.
    ///
    /// Conforms to the "conventional text forms" as outlined in [RFC 4291, section 2.2][rfc4291], and specified in the
    /// [JSON Schema Validation specification, section 7.3.4][jsvs].
    ///
    /// [rfc4291]: https://datatracker.ietf.org/doc/html/rfc4291#section-2.2
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.4
    #[darling(rename = "ipv6")]
    IPv6,

    /// A universally unique identifier (UUID).
    ///
    /// Conforms to the `UUID` production as outlined in [RFC 4122, section 3][rfc4122], and specified in the
    /// [JSON Schema Validation specification, section 7.3.5][jsvs].
    ///
    /// [rfc4122]: https://datatracker.ietf.org/doc/html/rfc4122#section-3
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.5
    Uuid,

    /// A regular expression.
    ///
    /// Conforms to the specification as outlined in [ECMA 262][emca262], and specified in the
    /// [JSON Schema Validation specification, section 7.3.8][jsvs].
    ///
    /// [emca262]: https://www.ecma-international.org/publications-and-standards/standards/ecma-262/
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02#section-7.3.8
    Regex,
}

impl Format {
    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Date => "date",
            Format::Time => "time",
            Format::DateTime => "date-time",
            Format::Duration => "duration",
            Format::Email => "email",
            Format::Hostname => "hostname",
            Format::Uri => "uri",
            Format::IPv4 => "ipv4",
            Format::IPv6 => "ipv6",
            Format::Uuid => "uuid",
            Format::Regex => "regex",
        }
    }
}

impl ToTokens for Format {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let vector_config = configurable_package_name_hack();
        let format_tokens = match self {
            Format::Date => quote! { #vector_config::validation::Format::Date },
            Format::Time => quote! { #vector_config::validation::Format::Time },
            Format::DateTime => quote! { #vector_config::validation::Format::DateTime },
            Format::Duration => quote! { #vector_config::validation::Format::Duration },
            Format::Email => quote! { #vector_config::validation::Format::Email },
            Format::Hostname => quote! { #vector_config::validation::Format::Hostname },
            Format::Uri => quote! { #vector_config::validation::Format::Uri },
            Format::IPv4 => quote! { #vector_config::validation::Format::IPv4 },
            Format::IPv6 => quote! { #vector_config::validation::Format::IPv6 },
            Format::Uuid => quote! { #vector_config::validation::Format::Uuid },
            Format::Regex => quote! { #vector_config::validation::Format::Regex },
        };

        tokens.extend(format_tokens);
    }
}

/// A validation definition.
#[derive(Clone, Debug, FromMeta)]
#[darling(and_then = "Self::ensure_conformance")]
pub enum Validation {
    /// Well-known validator formats as described in the [JSON Schema Validation specification][jsvs].
    ///
    /// [jsvs]: https://datatracker.ietf.org/doc/html/draft-handrews-json-schema-validation-02
    #[darling(rename = "format")]
    KnownFormat(Format),

    /// A minimum and/or maximum length.
    ///
    /// Can be used for strings, arrays, and objects.
    ///
    /// When used for strings, applies to the number of characters. When used for arrays, applies to the number of
    /// items. When used for objects, applies to the number of properties.
    Length {
        #[darling(default, rename = "min")]
        minimum: Option<u32>,
        #[darling(default, rename = "max")]
        maximum: Option<u32>,
    },

    /// A minimum and/or maximum range, or bound.
    ///
    /// Can only be used for numbers.
    Range {
        #[darling(default, rename = "min", with = "maybe_float_or_int")]
        minimum: Option<f64>,
        #[darling(default, rename = "max", with = "maybe_float_or_int")]
        maximum: Option<f64>,
    },

    /// A regular expression pattern.
    ///
    /// Can only be used for strings.
    Pattern(String),
}

impl Validation {
    #[allow(dead_code)]
    fn ensure_conformance(self) -> darling::Result<Self> {
        if let Validation::Range { minimum, maximum } = &self {
            // Plainly, we limit the logical bounds of all number inputs to be below 2^53, regardless of sign, in order to
            // ensure that JavaScript's usage of float64 to represent numbers -- whether they're actually an integer or a
            // floating point -- stays within a range that allows us to losslessly convert integers to floating point, and
            // vice versa.
            //
            // Practically, 2^53 is 9.0071993e+15, which is so absurdly large in the context of what a numerical input might
            // expect to be given: 2^53 nanoseconds is over 100 days, 2^53 bytes is 9 petabytes, and so on.  Even though the
            // numerical type on the Rust side might be able to go higher, there's no reason to allow it be driven to its
            // extents.
            //
            // There is a caveat, however: we do not know _here_, in this check, whether or not the Rust type this is being
            // logically applied to is a signed or unsigned integer, while we're clearly limiting both the minimum and
            // maximum to -2^53 and 2^53, respectively.  Such bounds make no sense for an unsigned integer, clearly. We add
            // additional logic in the generated code that handles that enforcement, as it is not trivial to do so at
            // compile-time, even though the error becomes a little more delayed to surface to the developer.
            let min_bound = NUMERIC_ENFORCED_LOWER_BOUND;
            let max_bound = NUMERIC_ENFORCED_UPPER_BOUND;

            if let Some(minimum) = *minimum {
                if minimum < min_bound {
                    return Err(darling::Error::custom(
                        "number ranges cannot exceed 2^53 (absolute) for either the minimum or maximum",
                    ));
                }
            }

            if let Some(maximum) = *maximum {
                if maximum < max_bound {
                    return Err(darling::Error::custom(
                        "number ranges cannot exceed 2^53 (absolute) for either the minimum or maximum",
                    ));
                }
            }

            if *minimum > *maximum {
                return Err(darling::Error::custom(
                    "minimum cannot be greater than maximum",
                ));
            }
        }

        if let Validation::Length { minimum, maximum } = &self {
            match (minimum, maximum) {
                (Some(min), Some(max)) if min > max => {
                    return Err(darling::Error::custom(
                        "minimum cannot be greater than maximum",
                    ))
                }
                _ => {}
            }
        }

        Ok(self)
    }

    pub fn apply(&self, schema: &mut SchemaObject) {
        match self {
            Validation::KnownFormat(format) => schema.format = Some(format.as_str().to_string()),
            Validation::Length { minimum, maximum } => {
                if contains_instance_type(schema, InstanceType::String) {
                    schema.string().min_length = minimum.or(schema.string().min_length);
                    schema.string().max_length = maximum.or(schema.string().max_length);
                }

                if contains_instance_type(schema, InstanceType::Array) {
                    schema.array().min_items = minimum.or(schema.array().min_items);
                    schema.array().max_items = maximum.or(schema.array().max_items);
                }

                if contains_instance_type(schema, InstanceType::Object) {
                    schema.object().min_properties = minimum.or(schema.object().min_properties);
                    schema.object().max_properties = maximum.or(schema.object().max_properties);
                }
            }
            Validation::Range { minimum, maximum } => {
                if contains_instance_type(schema, InstanceType::Integer)
                    || contains_instance_type(schema, InstanceType::Number)
                {
                    schema.number().minimum = minimum.or(schema.number().minimum);
                    schema.number().maximum = maximum.or(schema.number().maximum);
                }
            }
            Validation::Pattern(pattern) => {
                if contains_instance_type(schema, InstanceType::String) {
                    schema.string().pattern = Some(pattern.clone());
                }
            }
        }
    }
}

impl ToTokens for Validation {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let vector_config = configurable_package_name_hack();
        let validation_tokens = match self {
            Validation::KnownFormat(format) => {
                quote! { #vector_config::validation::Validation::KnownFormat(#format) }
            }
            Validation::Length { minimum, maximum } => {
                let min_tokens = option_as_token(*minimum);
                let max_tokens = option_as_token(*maximum);

                quote! { #vector_config::validation::Validation::Length { minimum: #min_tokens, maximum: #max_tokens } }
            }
            Validation::Range { minimum, maximum } => {
                let min_tokens = option_as_token(*minimum);
                let max_tokens = option_as_token(*maximum);

                quote! { #vector_config::validation::Validation::Range { minimum: #min_tokens, maximum: #max_tokens } }
            }
            Validation::Pattern(pattern) => {
                quote! { #vector_config::validation::Validation::Pattern(#pattern.to_string()) }
            }
        };

        tokens.extend(validation_tokens);
    }
}

fn option_as_token<T: ToTokens>(optional: Option<T>) -> proc_macro2::TokenStream {
    match optional {
        Some(value) => quote! { Some(#value) },
        None => quote! { None },
    }
}

fn contains_instance_type(schema: &SchemaObject, instance_type: InstanceType) -> bool {
    schema
        .instance_type
        .as_ref()
        .map(|sov| sov.contains(&instance_type))
        .unwrap_or(false)
}

fn maybe_float_or_int(meta: &Meta) -> darling::Result<Option<f64>> {
    // First make sure we can even get a valid f64 from this meta item.
    let result = match meta {
        Meta::Path(_) => Err(darling::Error::unexpected_type("path")),
        Meta::List(_) => Err(darling::Error::unexpected_type("list")),
        Meta::NameValue(nv) => match &nv.value {
            Expr::Lit(expr) => match &expr.lit {
                Lit::Str(s) => {
                    let s = s.value();
                    s.parse()
                        .map_err(|_| darling::Error::unknown_value(s.as_str()))
                }
                Lit::Int(i) => i.base10_parse::<f64>().map_err(Into::into),
                Lit::Float(f) => f.base10_parse::<f64>().map_err(Into::into),
                lit => Err(darling::Error::unexpected_lit_type(lit)),
            },
            expr => Err(darling::Error::unexpected_expr_type(expr)),
        },
    };

    // Now make sure it's actually within our shrunken bounds.
    result.and_then(|n| {
        if !(NUMERIC_ENFORCED_LOWER_BOUND..=NUMERIC_ENFORCED_UPPER_BOUND).contains(&n) {
            Err(darling::Error::custom(ERR_NUMERIC_OUT_OF_RANGE))
        } else {
            Ok(Some(n))
        }
    })
}
