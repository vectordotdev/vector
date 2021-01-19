mod util;

#[cfg(feature = "append")]
mod append;
#[cfg(feature = "assert")]
mod assert;
#[cfg(feature = "ceil")]
mod ceil;
#[cfg(feature = "compact")]
mod compact;
#[cfg(feature = "contains")]
mod contains;
#[cfg(feature = "decode_base64")]
mod decode_base64;
#[cfg(feature = "del")]
mod del;
#[cfg(feature = "downcase")]
mod downcase;
#[cfg(feature = "encode_base64")]
mod encode_base64;
#[cfg(feature = "encode_json")]
mod encode_json;
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
#[cfg(feature = "get_env_var")]
mod get_env_var;
#[cfg(feature = "includes")]
mod includes;
#[cfg(feature = "ip_cidr_contains")]
mod ip_cidr_contains;
#[cfg(feature = "ip_subnet")]
mod ip_subnet;
#[cfg(feature = "ip_to_ipv6")]
mod ip_to_ipv6;
#[cfg(feature = "ipv6_to_ipv4")]
mod ipv6_to_ipv4;
#[cfg(feature = "is_nullish")]
mod is_nullish;
#[cfg(feature = "length")]
mod length;
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
#[cfg(feature = "parse_aws_alb_log")]
mod parse_aws_alb_log;
#[cfg(feature = "parse_aws_cloudwatch_log_subscription_message")]
mod parse_aws_cloudwatch_log_subscription_message;
#[cfg(feature = "parse_aws_vpc_flow_log")]
mod parse_aws_vpc_flow_log;
#[cfg(feature = "parse_duration")]
mod parse_duration;
#[cfg(feature = "parse_grok")]
mod parse_grok;
#[cfg(feature = "parse_json")]
mod parse_json;
#[cfg(feature = "parse_key_value")]
mod parse_key_value;
#[cfg(feature = "parse_regex")]
mod parse_regex;
#[cfg(feature = "parse_regex_all")]
mod parse_regex_all;
#[cfg(feature = "parse_syslog")]
mod parse_syslog;
#[cfg(feature = "parse_timestamp")]
mod parse_timestamp;
#[cfg(feature = "parse_tokens")]
mod parse_tokens;
#[cfg(feature = "parse_url")]
mod parse_url;
#[cfg(feature = "push")]
mod push;
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
#[cfg(feature = "to_syslog_facility")]
mod to_syslog_facility;
#[cfg(feature = "to_syslog_level")]
mod to_syslog_level;
#[cfg(feature = "to_syslog_severity")]
mod to_syslog_severity;
#[cfg(feature = "to_timestamp")]
mod to_timestamp;
#[cfg(feature = "to_unix_timestamp")]
mod to_unix_timestamp;
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
#[cfg(feature = "append")]
pub use append::Append;
#[cfg(feature = "assert")]
pub use assert::Assert;
#[cfg(feature = "ceil")]
pub use ceil::Ceil;
#[cfg(feature = "compact")]
pub use compact::Compact;
#[cfg(feature = "contains")]
pub use contains::Contains;
#[cfg(feature = "decode_base64")]
pub use decode_base64::DecodeBase64;
#[cfg(feature = "del")]
pub use del::Del;
#[cfg(feature = "downcase")]
pub use downcase::Downcase;
#[cfg(feature = "encode_base64")]
pub use encode_base64::EncodeBase64;
#[cfg(feature = "encode_json")]
pub use encode_json::EncodeJson;
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
#[cfg(feature = "get_env_var")]
pub use get_env_var::GetEnvVar;
#[cfg(feature = "includes")]
pub use includes::Includes;
#[cfg(feature = "ip_cidr_contains")]
pub use ip_cidr_contains::IpCidrContains;
#[cfg(feature = "ip_subnet")]
pub use ip_subnet::IpSubnet;
#[cfg(feature = "ip_to_ipv6")]
pub use ip_to_ipv6::IpToIpv6;
#[cfg(feature = "ipv6_to_ipv4")]
pub use ipv6_to_ipv4::Ipv6ToIpV4;
#[cfg(feature = "is_nullish")]
pub use is_nullish::IsNullish;
#[cfg(feature = "length")]
pub use length::Length;
#[cfg(feature = "log")]
pub use log::Log;
#[cfg(feature = "merge")]
pub use merge::Merge;
#[cfg(feature = "now")]
pub use now::Now;
#[cfg(feature = "parse_aws_alb_log")]
pub use parse_aws_alb_log::ParseAwsAlbLog;
#[cfg(feature = "parse_aws_cloudwatch_log_subscription_message")]
pub use parse_aws_cloudwatch_log_subscription_message::ParseAwsCloudWatchLogSubscriptionMessage;
#[cfg(feature = "parse_aws_vpc_flow_log")]
pub use parse_aws_vpc_flow_log::ParseAwsVpcFlowLog;
#[cfg(feature = "parse_duration")]
pub use parse_duration::ParseDuration;
#[cfg(feature = "parse_grok")]
pub use parse_grok::ParseGrok;
#[cfg(feature = "parse_json")]
pub use parse_json::ParseJson;
#[cfg(feature = "parse_key_value")]
pub use parse_key_value::ParseKeyValue;
#[cfg(feature = "parse_regex")]
pub use parse_regex::ParseRegex;
#[cfg(feature = "parse_regex_all")]
pub use parse_regex_all::ParseRegexAll;
#[cfg(feature = "parse_syslog")]
pub use parse_syslog::ParseSyslog;
#[cfg(feature = "parse_timestamp")]
pub use parse_timestamp::ParseTimestamp;
#[cfg(feature = "parse_tokens")]
pub use parse_tokens::ParseTokens;
#[cfg(feature = "parse_url")]
pub use parse_url::ParseUrl;
#[cfg(feature = "push")]
pub use push::Push;
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
#[cfg(feature = "to_syslog_facility")]
pub use to_syslog_facility::ToSyslogFacility;
#[cfg(feature = "to_syslog_level")]
pub use to_syslog_level::ToSyslogLevel;
#[cfg(feature = "to_syslog_severity")]
pub use to_syslog_severity::ToSyslogSeverity;
#[cfg(feature = "to_timestamp")]
pub use to_timestamp::ToTimestamp;
#[cfg(feature = "to_unix_timestamp")]
pub use to_unix_timestamp::ToUnixTimestamp;
#[cfg(feature = "truncate")]
pub use truncate::Truncate;
#[cfg(feature = "upcase")]
pub use upcase::Upcase;
#[cfg(feature = "uuid_v4")]
pub use uuid_v4::UuidV4;

pub fn all() -> Vec<Box<dyn remap::Function>> {
    vec![
        #[cfg(feature = "append")]
        Box::new(Append),
        #[cfg(feature = "assert")]
        Box::new(Assert),
        #[cfg(feature = "ceil")]
        Box::new(Ceil),
        #[cfg(feature = "compact")]
        Box::new(Compact),
        #[cfg(feature = "contains")]
        Box::new(Contains),
        #[cfg(feature = "decode_base64")]
        Box::new(DecodeBase64),
        #[cfg(feature = "del")]
        Box::new(Del),
        #[cfg(feature = "downcase")]
        Box::new(Downcase),
        #[cfg(feature = "encode_base64")]
        Box::new(EncodeBase64),
        #[cfg(feature = "encode_json")]
        Box::new(EncodeJson),
        #[cfg(feature = "ends_with")]
        Box::new(EndsWith),
        #[cfg(feature = "exists")]
        Box::new(Exists),
        #[cfg(feature = "parse_regex")]
        Box::new(ParseRegex),
        #[cfg(feature = "parse_regex_all")]
        Box::new(ParseRegexAll),
        #[cfg(feature = "flatten")]
        Box::new(Flatten),
        #[cfg(feature = "floor")]
        Box::new(Floor),
        #[cfg(feature = "format_number")]
        Box::new(FormatNumber),
        #[cfg(feature = "format_timestamp")]
        Box::new(FormatTimestamp),
        #[cfg(feature = "get_env_var")]
        Box::new(GetEnvVar),
        #[cfg(feature = "includes")]
        Box::new(Includes),
        #[cfg(feature = "ip_cidr_contains")]
        Box::new(IpCidrContains),
        #[cfg(feature = "ip_subnet")]
        Box::new(IpSubnet),
        #[cfg(feature = "ip_to_ipv6")]
        Box::new(IpToIpv6),
        #[cfg(feature = "ipv6_to_ipv4")]
        Box::new(Ipv6ToIpV4),
        #[cfg(feature = "is_nullish")]
        Box::new(IsNullish),
        #[cfg(feature = "length")]
        Box::new(Length),
        #[cfg(feature = "log")]
        Box::new(Log),
        #[cfg(feature = "md5")]
        Box::new(Md5),
        #[cfg(feature = "merge")]
        Box::new(Merge),
        #[cfg(feature = "now")]
        Box::new(Now),
        #[cfg(feature = "parse_aws_alb_log")]
        Box::new(ParseAwsAlbLog),
        #[cfg(feature = "parse_aws_cloudwatch_log_subscription_message")]
        Box::new(ParseAwsCloudWatchLogSubscriptionMessage),
        #[cfg(feature = "parse_aws_vpc_flow_log")]
        Box::new(ParseAwsVpcFlowLog),
        #[cfg(feature = "parse_duration")]
        Box::new(ParseDuration),
        #[cfg(feature = "parse_grok")]
        Box::new(ParseGrok),
        #[cfg(feature = "parse_json")]
        Box::new(ParseJson),
        #[cfg(feature = "parse_key_value")]
        Box::new(ParseKeyValue),
        #[cfg(feature = "parse_syslog")]
        Box::new(ParseSyslog),
        #[cfg(feature = "parse_timestamp")]
        Box::new(ParseTimestamp),
        #[cfg(feature = "parse_tokens")]
        Box::new(ParseTokens),
        #[cfg(feature = "parse_url")]
        Box::new(ParseUrl),
        #[cfg(feature = "push")]
        Box::new(Push),
        #[cfg(feature = "match")]
        Box::new(Match),
        #[cfg(feature = "redact")]
        Box::new(Redact),
        #[cfg(feature = "replace")]
        Box::new(Replace),
        #[cfg(feature = "round")]
        Box::new(Round),
        #[cfg(feature = "sha1")]
        Box::new(Sha1),
        #[cfg(feature = "sha2")]
        Box::new(Sha2),
        #[cfg(feature = "sha3")]
        Box::new(Sha3),
        #[cfg(feature = "slice")]
        Box::new(Slice),
        #[cfg(feature = "split")]
        Box::new(Split),
        #[cfg(feature = "starts_with")]
        Box::new(StartsWith),
        #[cfg(feature = "strip_ansi_escape_codes")]
        Box::new(StripAnsiEscapeCodes),
        #[cfg(feature = "strip_whitespace")]
        Box::new(StripWhitespace),
        #[cfg(feature = "to_bool")]
        Box::new(ToBool),
        #[cfg(feature = "to_float")]
        Box::new(ToFloat),
        #[cfg(feature = "to_int")]
        Box::new(ToInt),
        #[cfg(feature = "to_syslog_facility")]
        Box::new(ToSyslogFacility),
        #[cfg(feature = "to_syslog_level")]
        Box::new(ToSyslogLevel),
        #[cfg(feature = "to_syslog_severity")]
        Box::new(ToSyslogSeverity),
        #[cfg(feature = "to_string")]
        Box::new(ToString),
        #[cfg(feature = "to_timestamp")]
        Box::new(ToTimestamp),
        #[cfg(feature = "to_unix_timestamp")]
        Box::new(ToUnixTimestamp),
        #[cfg(feature = "truncate")]
        Box::new(Truncate),
        #[cfg(feature = "upcase")]
        Box::new(Upcase),
        #[cfg(feature = "uuid_v4")]
        Box::new(UuidV4),
    ]
}
