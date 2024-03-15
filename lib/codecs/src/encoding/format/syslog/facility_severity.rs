use std::str::FromStr;
use strum::{FromRepr, EnumString};

#[derive(Default, Debug)]
struct Pri {
    facility: Facility,
    severity: Severity,
}

impl Pri {
    fn from_str_variants(facility_variant: &str, severity_variant: &str) -> Self {
        // The original PR had `deserialize_*()` methods parsed a value to a `u8` or stored a field key as a `String`
        // Later the equivalent `get_num_*()` method would retrieve the `u8` value or lookup the field key for the actual value,
        // otherwise it'd fallback to the default Facility/Severity value.
        // This approach instead parses a string of the name or ordinal representation,
        // any reference via field key lookup should have already happened by this point.
        let facility = Facility::into_variant(&facility_variant).unwrap_or(Facility::User);
        let severity = Severity::into_variant(&severity_variant).unwrap_or(Severity::Informational);

        Self {
            facility,
            severity,
        }
    }

    // The last paragraph describes how to compose the enums into `PRIVAL`:
    // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2.1
    fn encode(&self) -> String {
        let prival = (self.facility as u8 * 8) + self.severity as u8;
        ["<", &prival.to_string(), ">"].concat()
    }
}

// Facility + Severity mapping from Name => Ordinal number:

/// Syslog facility
#[derive(Default, Debug, EnumString, FromRepr, Copy, Clone)]
#[strum(serialize_all = "kebab-case")]
enum Facility {
    Kern = 0,
    #[default]
    User = 1,
    Mail = 2,
    Daemon = 3,
    Auth = 4,
    Syslog = 5,
    LPR = 6,
    News = 7,
    UUCP = 8,
    Cron = 9,
    AuthPriv = 10,
    FTP = 11,
    NTP = 12,
    Security = 13,
    Console = 14,
    SolarisCron = 15,
    Local0 = 16,
    Local1 = 17,
    Local2 = 18,
    Local3 = 19,
    Local4 = 20,
    Local5 = 21,
    Local6 = 22,
    Local7 = 23,
}

/// Syslog severity
#[derive(Default, Debug, EnumString, FromRepr, Copy, Clone)]
#[strum(serialize_all = "kebab-case")]
enum Severity {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    #[default]
    Informational = 6,
    Debug = 7,
}

// Additionally support variants from string-based integers:
// Parse a string name, with fallback for parsing a string ordinal number.
impl Facility {
    fn into_variant(variant_name: &str) -> Option<Self> {
        let s = variant_name.to_ascii_lowercase();

        s.parse::<usize>().map_or_else(
            |_| Self::from_str(&s).ok(),
            |num| Self::from_repr(num),
        )
    }
}

// NOTE: The `strum` crate does not provide traits,
// requiring copy/paste of the prior impl instead.
impl Severity {
    fn into_variant(variant_name: &str) -> Option<Self> {
        let s = variant_name.to_ascii_lowercase();

        s.parse::<usize>().map_or_else(
            |_| Self::from_str(&s).ok(),
            |num| Self::from_repr(num),
        )
    }
}
