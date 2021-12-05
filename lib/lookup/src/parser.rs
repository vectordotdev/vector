use crate::Lookup;
use lalrpop_util::lalrpop_mod;

lalrpop_mod!(
    #[allow(clippy::all)]
    #[allow(unused)]
    path
);

/// Parses the string as a lookup path.
pub fn parse_lookup(s: &str) -> Result<Lookup, String> {
    path::LookupParser::new()
        .parse(s)
        .map_err(|err| format!("{}", err))
}
