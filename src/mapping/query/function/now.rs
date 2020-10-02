use super::prelude::*;
use chrono::Utc;

#[derive(Debug)]
pub(in crate::mapping) struct NowFn {}

impl NowFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new() -> Self {
        Self {}
    }
}

impl Function for NowFn {
    fn execute(&self, _: &Event) -> Result<Value> {
        Ok(Value::Timestamp(Utc::now()))
    }
}

impl TryFrom<ArgumentList> for NowFn {
    type Error = String;

    fn try_from(_: ArgumentList) -> Result<Self> {
        Ok(Self {})
    }
}
