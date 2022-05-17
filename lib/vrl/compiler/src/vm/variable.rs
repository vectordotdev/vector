use lookup::LookupBuf;

#[cfg(feature = "expr-assignment")]
use crate::expression::assignment::Target;

#[derive(Clone, Debug, PartialEq)]
pub enum Variable {
    Internal(crate::parser::Ident, LookupBuf),
    External(LookupBuf),
    Stack(LookupBuf),
    None,
}

#[cfg(feature = "expr-assignment")]
impl From<&Target> for Variable {
    fn from(target: &Target) -> Self {
        match target {
            Target::External(path) => Variable::External(path.clone()),
            Target::Noop => Variable::None,
            Target::Internal(ident, path) => Variable::Internal(ident.clone(), path.clone()),
        }
    }
}
