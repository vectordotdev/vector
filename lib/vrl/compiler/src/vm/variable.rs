use crate::expression::assignment::Target;
use lookup::LookupBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Variable {
    Internal(crate::parser::Ident, Option<lookup::LookupBuf>),
    External(lookup::LookupBuf),
    None,
}

impl From<&Target> for Variable {
    fn from(target: &Target) -> Self {
        match target {
            Target::External(Some(path)) => Variable::External(path.clone()),
            Target::External(None) => Variable::External(LookupBuf::root()),
            Target::Noop => Variable::None,
            Target::Internal(ident, path) => Variable::Internal(ident.clone(), path.clone()),
        }
    }
}
