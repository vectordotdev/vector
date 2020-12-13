mod util;

#[cfg(feature = "assert")]
mod assert;
#[cfg(feature = "ceil")]
mod ceil;
#[cfg(feature = "compact")]
mod compact;
#[cfg(feature = "contains")]
mod contains;
#[cfg(feature = "del")]
mod del;
#[cfg(feature = "downcase")]
mod downcase;
#[cfg(feature = "ends_with")]
mod ends_with;
#[cfg(feature = "exists")]
mod exists;
#[cfg(feature = "flatten")]
mod flatten;
#[cfg(feature = "floor")]
mod floor;
#[cfg(feature = "format_number")]
mod format_number;
#[cfg(feature = "format_timestamp")]
mod format_timestamp;
#[cfg(feature = "ip_cidr_contains")]
mod ip_cidr_contains;
#[cfg(feature = "ip_subnet")]
mod ip_subnet;
#[cfg(feature = "ip_to_ipv6")]
mod ip_to_ipv6;
#[cfg(feature = "ipv6_to_ipv4")]
mod ipv6_to_ipv4;
#[cfg(feature = "log")]
mod log;
#[cfg(feature = "match")]
mod r#match;
#[cfg(feature = "md5")]
mod md5;
#[cfg(feature = "merge")]
mod merge;
#[cfg(feature = "now")]
mod now;
#[cfg(feature = "only_fields")]
mod only_fields;
#[cfg(feature = "parse_duration")]
mod parse_duration;
#[cfg(feature = "parse_grok")]
mod parse_grok;
#[cfg(feature = "parse_json")]
mod parse_json;
#[cfg(feature = "parse_syslog")]
mod parse_syslog;
#[cfg(feature = "parse_timestamp")]
mod parse_timestamp;
#[cfg(feature = "parse_url")]
mod parse_url;
#[cfg(feature = "redact")]
mod redact;
#[cfg(feature = "replace")]
mod replace;
#[cfg(feature = "round")]
mod round;
#[cfg(feature = "sha1")]
mod sha1;
#[cfg(feature = "sha2")]
mod sha2;
#[cfg(feature = "sha3")]
mod sha3;
#[cfg(feature = "slice")]
mod slice;
#[cfg(feature = "split")]
mod split;
#[cfg(feature = "starts_with")]
mod starts_with;
#[cfg(feature = "strip_ansi_escape_codes")]
mod strip_ansi_escape_codes;
#[cfg(feature = "strip_whitespace")]
mod strip_whitespace;
#[cfg(feature = "to_bool")]
mod to_bool;
#[cfg(feature = "to_float")]
mod to_float;
#[cfg(feature = "to_int")]
mod to_int;
#[cfg(feature = "to_string")]
mod to_string;
#[cfg(feature = "to_timestamp")]
mod to_timestamp;
#[cfg(feature = "tokenize")]
mod tokenize;
#[cfg(feature = "truncate")]
mod truncate;
#[cfg(feature = "upcase")]
mod upcase;
#[cfg(feature = "uuid_v4")]
mod uuid_v4;

// -----------------------------------------------------------------------------

#[cfg(feature = "md5")]
pub use crate::md5::Md5;
#[cfg(feature = "sha1")]
pub use crate::sha1::Sha1;
#[cfg(feature = "assert")]
pub use assert::Assert;
#[cfg(feature = "ceil")]
pub use ceil::Ceil;
#[cfg(feature = "compact")]
pub use compact::Compact;
#[cfg(feature = "contains")]
pub use contains::Contains;
#[cfg(feature = "del")]
pub use del::Del;
#[cfg(feature = "downcase")]
pub use downcase::Downcase;
#[cfg(feature = "ends_with")]
pub use ends_with::EndsWith;
#[cfg(feature = "exists")]
pub use exists::Exists;
#[cfg(feature = "flatten")]
pub use flatten::Flatten;
#[cfg(feature = "floor")]
pub use floor::Floor;
#[cfg(feature = "format_number")]
pub use format_number::FormatNumber;
#[cfg(feature = "format_timestamp")]
pub use format_timestamp::FormatTimestamp;
#[cfg(feature = "ip_cidr_contains")]
pub use ip_cidr_contains::IpCidrContains;
#[cfg(feature = "ip_subnet")]
pub use ip_subnet::IpSubnet;
#[cfg(feature = "ip_to_ipv6")]
pub use ip_to_ipv6::IpToIpv6;
#[cfg(feature = "ipv6_to_ipv4")]
pub use ipv6_to_ipv4::Ipv6ToIpV4;
#[cfg(feature = "log")]
pub use log::Log;
#[cfg(feature = "merge")]
pub use merge::Merge;
#[cfg(feature = "now")]
pub use now::Now;
#[cfg(feature = "only_fields")]
pub use only_fields::OnlyFields;
#[cfg(feature = "parse_duration")]
pub use parse_duration::ParseDuration;
#[cfg(feature = "parse_grok")]
pub use parse_grok::ParseGrok;
#[cfg(feature = "parse_json")]
pub use parse_json::ParseJson;
#[cfg(feature = "parse_syslog")]
pub use parse_syslog::ParseSyslog;
#[cfg(feature = "parse_timestamp")]
pub use parse_timestamp::ParseTimestamp;
#[cfg(feature = "parse_url")]
pub use parse_url::ParseUrl;
#[cfg(feature = "match")]
pub use r#match::Match;
#[cfg(feature = "redact")]
pub use redact::Redact;
#[cfg(feature = "replace")]
pub use replace::Replace;
#[cfg(feature = "round")]
pub use round::Round;
#[cfg(feature = "sha2")]
pub use sha2::Sha2;
#[cfg(feature = "sha3")]
pub use sha3::Sha3;
#[cfg(feature = "slice")]
pub use slice::Slice;
#[cfg(feature = "split")]
pub use split::Split;
#[cfg(feature = "starts_with")]
pub use starts_with::StartsWith;
#[cfg(feature = "strip_ansi_escape_codes")]
pub use strip_ansi_escape_codes::StripAnsiEscapeCodes;
#[cfg(feature = "strip_whitespace")]
pub use strip_whitespace::StripWhitespace;
#[cfg(feature = "to_bool")]
pub use to_bool::ToBool;
#[cfg(feature = "to_float")]
pub use to_float::ToFloat;
#[cfg(feature = "to_int")]
pub use to_int::ToInt;
#[cfg(feature = "to_string")]
pub use to_string::ToString;
#[cfg(feature = "to_timestamp")]
pub use to_timestamp::ToTimestamp;
#[cfg(feature = "tokenize")]
pub use tokenize::Tokenize;
#[cfg(feature = "truncate")]
pub use truncate::Truncate;
#[cfg(feature = "upcase")]
pub use upcase::Upcase;
#[cfg(feature = "uuid_v4")]
pub use uuid_v4::UuidV4;

pub fn all() -> Vec<Box<dyn remap::Function>> {
    let mut functions: Vec<Box<dyn remap::Function>> = vec![];

    #[cfg(feature = "md5")]
    functions.push(Box::new(Md5));
    #[cfg(feature = "sha1")]
    functions.push(Box::new(Sha1));
    #[cfg(feature = "assert")]
    functions.push(Box::new(Assert));
    #[cfg(feature = "ceil")]
    functions.push(Box::new(Ceil));
    #[cfg(feature = "compact")]
    functions.push(Box::new(Compact));
    #[cfg(feature = "contains")]
    functions.push(Box::new(Contains));
    #[cfg(feature = "del")]
    functions.push(Box::new(Del));
    #[cfg(feature = "downcase")]
    functions.push(Box::new(Downcase));
    #[cfg(feature = "ends_with")]
    functions.push(Box::new(EndsWith));
    #[cfg(feature = "exists")]
    functions.push(Box::new(Exists));
    #[cfg(feature = "flatten")]
    functions.push(Box::new(Flatten));
    #[cfg(feature = "floor")]
    functions.push(Box::new(Floor));
    #[cfg(feature = "format_number")]
    functions.push(Box::new(FormatNumber));
    #[cfg(feature = "format_timestamp")]
    functions.push(Box::new(FormatTimestamp));
    #[cfg(feature = "ip_cidr_contains")]
    functions.push(Box::new(IpCidrContains));
    #[cfg(feature = "ip_subnet")]
    functions.push(Box::new(IpSubnet));
    #[cfg(feature = "ip_to_ipv6")]
    functions.push(Box::new(IpToIpv6));
    #[cfg(feature = "ipv6_to_ipv4")]
    functions.push(Box::new(Ipv6ToIpV4));
    #[cfg(feature = "log")]
    functions.push(Box::new(Log));
    #[cfg(feature = "merge")]
    functions.push(Box::new(Merge));
    #[cfg(feature = "now")]
    functions.push(Box::new(Now));
    #[cfg(feature = "only_fields")]
    functions.push(Box::new(OnlyFields));
    #[cfg(feature = "parse_duration")]
    functions.push(Box::new(ParseDuration));
    #[cfg(feature = "parse_grok")]
    functions.push(Box::new(ParseGrok));
    #[cfg(feature = "parse_json")]
    functions.push(Box::new(ParseJson));
    #[cfg(feature = "parse_syslog")]
    functions.push(Box::new(ParseSyslog));
    #[cfg(feature = "parse_timestamp")]
    functions.push(Box::new(ParseTimestamp));
    #[cfg(feature = "parse_url")]
    functions.push(Box::new(ParseUrl));
    #[cfg(feature = "match")]
    functions.push(Box::new(Match));
    #[cfg(feature = "redact")]
    functions.push(Box::new(Redact));
    #[cfg(feature = "replace")]
    functions.push(Box::new(Replace));
    #[cfg(feature = "round")]
    functions.push(Box::new(Round));
    #[cfg(feature = "sha2")]
    functions.push(Box::new(Sha2));
    #[cfg(feature = "sha3")]
    functions.push(Box::new(Sha3));
    #[cfg(feature = "slice")]
    functions.push(Box::new(Slice));
    #[cfg(feature = "split")]
    functions.push(Box::new(Split));
    #[cfg(feature = "starts_with")]
    functions.push(Box::new(StartsWith));
    #[cfg(feature = "strip_ansi_escape_codes")]
    functions.push(Box::new(StripAnsiEscapeCodes));
    #[cfg(feature = "strip_whitespace")]
    functions.push(Box::new(StripWhitespace));
    #[cfg(feature = "to_bool")]
    functions.push(Box::new(ToBool));
    #[cfg(feature = "to_float")]
    functions.push(Box::new(ToFloat));
    #[cfg(feature = "to_int")]
    functions.push(Box::new(ToInt));
    #[cfg(feature = "to_string")]
    functions.push(Box::new(ToString));
    #[cfg(feature = "to_timestamp")]
    functions.push(Box::new(ToTimestamp));
    #[cfg(feature = "tokenize")]
    functions.push(Box::new(Tokenize));
    #[cfg(feature = "truncate")]
    functions.push(Box::new(Truncate));
    #[cfg(feature = "upcase")]
    functions.push(Box::new(Upcase));
    #[cfg(feature = "uuid_v4")]
    functions.push(Box::new(UuidV4));

    functions
}
