pub(crate) mod function;

pub use function::*;
use lazy_static::lazy_static;

lazy_static! {
    // List of immutable functions that can be loaded into a remap-lang program.
    pub(crate) static ref FUNCTIONS: Vec<Box<dyn remap::Function>> = vec![
        Box::new(Split),
        Box::new(ToString),
        Box::new(ToInt),
        Box::new(ToFloat),
        Box::new(ToBool),
        Box::new(ToTimestamp),
        Box::new(Upcase),
        Box::new(Downcase),
        Box::new(UuidV4),
        Box::new(Sha1),
        Box::new(Md5),
        Box::new(Now),
        Box::new(FormatTimestamp),
        Box::new(Contains),
        Box::new(StartsWith),
        Box::new(EndsWith),
        Box::new(Slice),
        Box::new(Tokenize),
        Box::new(Sha2),
        Box::new(Sha3),
        Box::new(ParseDuration),
        Box::new(FormatNumber),
        Box::new(ParseUrl),
        Box::new(Ceil),
        Box::new(Floor),
        Box::new(Round),
        Box::new(ParseGrok),
        Box::new(ParseSyslog),
        Box::new(ParseTimestamp),
        Box::new(ParseJson),
        Box::new(Truncate),
        Box::new(StripWhitespace),
        Box::new(StripAnsiEscapeCodes),
        Box::new(Match),
        Box::new(Replace),
        Box::new(IpToIpv6),
        Box::new(Ipv6ToIpV4),
        Box::new(IpCidrContains),
        Box::new(IpSubnet),
        Box::new(Exists),
        Box::new(Compact),
    ];

    // List of both mutable, and immutable functions that can be loaded into a
    // remap-lang program.
    pub(crate) static ref FUNCTIONS_MUT: Vec<Box<dyn remap::Function>> = {
        let mut vec: Vec<Box<dyn remap::Function>> = vec![Box::new(Del), Box::new(OnlyFields)];

        vec.extend(FUNCTIONS.clone());
        vec
    };
}
