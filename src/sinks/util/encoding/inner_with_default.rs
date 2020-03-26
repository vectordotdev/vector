use crate::sinks::util::encoding::TimestampFormat;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Default, Eq, PartialEq, Clone)]
pub struct InnerWithDefault<E: Default> {
    #[serde(default)]
    pub(crate) codec: E,
    #[serde(default)]
    pub(crate) only_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(crate) except_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(crate) timestamp_format: Option<TimestampFormat>,
}
