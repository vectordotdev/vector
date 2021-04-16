use crate::Lookup;
use lalrpop_util::lalrpop_mod;

lalrpop_mod!(pub path);

pub(crate) fn parse_lookup<'a>(s: &'a str) -> Result<Lookup<'a>, String> {
    path::LookupParser::new()
        .parse(s)
        .map_err(|err| format!("{}", err))
}

