use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstancePath(Vec<String>);

impl InstancePath {
    pub fn rooted() -> Self {
        Self(Vec::new())
    }

    pub fn push<S>(&self, segment: S) -> Self
    where
        S: Into<InstancePath>,
    {
        let path = segment.into();
        let mut segments = self.0.clone();
        segments.extend(path.0);

        Self(segments)
    }

    pub fn lookup<'a>(&self, instance: &'a Value) -> Option<&'a Value> {
        let pointer = Some(instance);

        pointer.and_then(|v| {
            let mut pointer = v;
            for segment in &self.0 {
                match &pointer[segment] {
                    Value::Null => return None,
                    value => pointer = value,
                }
            }

            if pointer.is_null() {
                None
            } else {
                Some(pointer)
            }
        })
    }
}

impl Default for InstancePath {
    fn default() -> Self {
        Self::rooted()
    }
}

impl<'a> From<&'a str> for InstancePath {
    fn from(value: &'a str) -> Self {
        Self(vec![value.to_string()])
    }
}

impl<'a> From<&'a [&str]> for InstancePath {
    fn from(value: &'a [&str]) -> Self {
        Self(value.iter().map(|s| s.to_string()).collect())
    }
}
