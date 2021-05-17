use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum SourceFramer {}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SourceFramers(Vec<SourceFramer>);

impl From<Vec<SourceFramer>> for SourceFramers {
    fn from(framers: Vec<SourceFramer>) -> Self {
        Self(framers)
    }
}
