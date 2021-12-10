#[derive(Clone, Debug, PartialEq)]
pub enum Variable {
    Internal,
    External(lookup::LookupBuf),
}
