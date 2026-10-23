//! Structural configuration merging. Scalar contributions stay at their field until
//! substitution and schema-guided conversion have established their types.

use indexmap::IndexMap;
use serde_json::Value;

use super::representation::ConfigMap;

#[derive(Clone, Debug)]
pub(super) enum ConfigNode {
    Object(IndexMap<String, Self>),
    Array(Vec<Self>),
    /// Nonempty, in merge order. These are definitions of one scalar, not an array.
    Scalar(Vec<Value>),
}

impl From<Value> for ConfigNode {
    fn from(value: Value) -> Self {
        match value {
            Value::Object(map) => Self::Object(
                map.into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect(),
            ),
            Value::Array(values) => Self::Array(values.into_iter().map(Self::from).collect()),
            value => Self::Scalar(vec![value]),
        }
    }
}

impl ConfigNode {
    pub(super) fn object() -> Self {
        Self::Object(IndexMap::new())
    }

    pub(super) fn from_map(map: ConfigMap) -> Self {
        Value::Object(map).into()
    }

    pub(super) fn fields(&self) -> &IndexMap<String, Self> {
        match self {
            Self::Object(fields) => fields,
            _ => unreachable!("configuration documents are objects"),
        }
    }

    pub(super) fn into_fields(self) -> IndexMap<String, Self> {
        match self {
            Self::Object(fields) => fields,
            _ => unreachable!("configuration documents are objects"),
        }
    }

    pub(super) fn merge(&mut self, other: Self) -> Result<(), Vec<String>> {
        self.merge_at(other, &mut Vec::new())
            .map_err(|error| vec![error])
    }

    fn merge_at(&mut self, other: Self, path: &mut Vec<String>) -> Result<(), String> {
        match (self, other) {
            (Self::Object(fields), Self::Object(other)) => {
                for (name, value) in other {
                    // Match serde_json::Map::remove's ordering, including same-stem files.
                    let value = if let Some(mut existing) = fields.swap_remove(&name) {
                        path.push(name.clone());
                        let result = existing.merge_at(value, path);
                        path.pop();
                        result?;
                        existing
                    } else {
                        value
                    };
                    fields.insert(name, value);
                }
                Ok(())
            }
            (Self::Array(values), Self::Array(other)) => {
                values.extend(other);
                Ok(())
            }
            (Self::Scalar(values), Self::Scalar(other)) => {
                let previous = values.last().expect("scalar has a contribution");
                let next = other.first().expect("scalar has a contribution");
                if !previous.is_string() && !next.is_string() && kind(previous) != kind(next) {
                    return Err(incompatible(path, previous, next));
                }
                values.extend(other);
                Ok(())
            }
            (left, right) => Err(incompatible(path, &left.latest(), &right.latest())),
        }
    }

    /// The final definitions provide the complete context for choosing schema variants.
    pub(super) fn latest(&self) -> Value {
        match self {
            Self::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(key, value)| (key.clone(), value.latest()))
                    .collect(),
            ),
            Self::Array(values) => Value::Array(values.iter().map(Self::latest).collect()),
            Self::Scalar(values) => values.last().expect("scalar has a contribution").clone(),
        }
    }

    /// Raw loading uses the original scalar types, without schema conversion.
    pub(super) fn raw(&self) -> Result<Value, Vec<String>> {
        self.check_raw_merges()?;
        Ok(self.latest())
    }

    pub(super) fn check_raw_merges(&self) -> Result<(), Vec<String>> {
        self.check_raw_merges_at(&mut Vec::new())
            .map_err(|error| vec![error])
    }

    fn check_raw_merges_at(&self, path: &mut Vec<String>) -> Result<(), String> {
        match self {
            Self::Object(fields) => {
                for (key, node) in fields {
                    path.push(key.clone());
                    let result = node.check_raw_merges_at(path);
                    path.pop();
                    result?;
                }
            }
            Self::Array(values) => {
                for (index, node) in values.iter().enumerate() {
                    path.push(index.to_string());
                    let result = node.check_raw_merges_at(path);
                    path.pop();
                    result?;
                }
            }
            Self::Scalar(values) => {
                for pair in values.windows(2) {
                    if kind(&pair[0]) != kind(&pair[1]) {
                        return Err(incompatible(path, &pair[0], &pair[1]));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn at_path_mut(&mut self, path: &[String]) -> Option<&mut Self> {
        path.iter().try_fold(self, |node, key| match node {
            Self::Object(fields) => fields.get_mut(key),
            Self::Array(values) => key
                .parse::<usize>()
                .ok()
                .and_then(|index| values.get_mut(index)),
            Self::Scalar(_) => None,
        })
    }

    /// Check only scalar merge compatibility. Ordinary strings and singleton-list
    /// conversions still replace, while original arrays concatenate during assembly.
    pub(super) fn resolve_scalar_history(
        &mut self,
        resolved: &Value,
        path: &[String],
        mut convert: impl FnMut(&mut Value) -> Result<(), String>,
    ) -> Result<(), String> {
        let Self::Scalar(values) = self else {
            return Ok(());
        };
        if values.len() < 2 || !values.iter().any(Value::is_string) {
            return Ok(());
        }
        let all_strings = values.iter().all(Value::is_string);
        if all_strings && (resolved.is_string() || resolved.is_array() || resolved.is_object()) {
            return Ok(());
        }
        for original in values {
            let mut converted = original.clone();
            convert(&mut converted)?;
            if converted.is_array()
                || converted.is_object()
                || (!original.is_string() && converted.is_string())
                || kind(&converted) != kind(resolved)
            {
                return Err(format!(
                    "Incompatible types at path \"$.{}\" after scalar coercion.",
                    path.join(".")
                ));
            }
            *original = converted;
        }
        Ok(())
    }

    pub(super) fn visit_strings(&self, visit: &mut impl FnMut(&str)) {
        match self {
            Self::Object(fields) => fields.values().for_each(|node| node.visit_strings(visit)),
            Self::Array(values) => values.iter().for_each(|node| node.visit_strings(visit)),
            Self::Scalar(values) => values.iter().filter_map(Value::as_str).for_each(visit),
        }
    }

    pub(super) fn interpolate(
        &mut self,
        interpolate: &impl Fn(&str) -> Result<String, Vec<String>>,
    ) -> Result<(), Vec<String>> {
        fn visit(
            node: &mut ConfigNode,
            interpolate: &impl Fn(&str) -> Result<String, Vec<String>>,
            errors: &mut Vec<String>,
        ) {
            match node {
                ConfigNode::Object(fields) => fields
                    .values_mut()
                    .for_each(|node| visit(node, interpolate, errors)),
                ConfigNode::Array(values) => values
                    .iter_mut()
                    .for_each(|node| visit(node, interpolate, errors)),
                ConfigNode::Scalar(values) => {
                    for value in values {
                        if let Value::String(value) = value {
                            match interpolate(value) {
                                Ok(updated) => *value = updated,
                                Err(mut failures) => errors.append(&mut failures),
                            }
                        }
                    }
                }
            }
        }
        let mut errors = Vec::new();
        visit(self, interpolate, &mut errors);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn incompatible(path: &[String], previous: &Value, next: &Value) -> String {
    let path = if path.is_empty() {
        "$".to_owned()
    } else {
        format!("$.{}", path.join("."))
    };
    format!(
        "Incompatible types at path \"{path}\", expected \"{}\" received \"{}\".",
        kind(previous),
        kind(next)
    )
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "float",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "table",
    }
}
