#[derive(Clone, Debug, PartialEq)]
pub enum Variable {
    Internal(crate::parser::Ident, Option<lookup::LookupBuf>),
    External(lookup::LookupBuf),
}
