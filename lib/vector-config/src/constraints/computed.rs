use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::path::InstancePath;

/// A set of computed constraints for a component.
///
/// Constraints represent a computation of a value based on a given input, specifically, an input
/// representing the configuration of the component. In many cases, behavior of a component is
/// relatively fixed: enabling a specific setting might not change any other external behavior about
/// the sink. However, in some cases, configuring a component in a certain way can change the
/// external behavior in a way that is not linear.
///
/// As an example, a source normally has a single, unnamed output. This means that it is referenced
/// in the inputs of transforms/sinks by its component ID and nothing else. However, some sources
/// may have multiple outputs, such that each named output is referenced as such: `<component
/// ID>.<output ID>`.
///
/// If a sink wanted to optionally enable having named outputs, instead of just the default output,
/// there isn't a great way to indicate that purely in the schema for configuring the component. It
/// has no natural equivalency to the raw definitions of fields and their type. This is where
/// constraints come in.
///
/// Constraints can be declared in a way that allows describing a computed value based on the input,
/// so using our example, there might be a field called `enable_multi_output`. We could add a
/// constraint that checks if the given input has a field called `enable_multi_output`, and if it is
/// set to `true`, return an array of output IDs as the result of the "computation". When this field
/// isn't present, or is set to `false`, it can simply return nothing, indicating the default
/// behavior.
///
/// This means that we can accurately model relatively straightforward constraints -- compute a
/// value if a field equals a specific value, or derive a value based on the values of other fields,
/// and so on -- with simple statements. Even further, because these constraints, and the primitives
/// used to build them, can themselves be serialized, we can include the constraints directly in a
/// schema. Once parsing the schema later on, the same code can be used to load the constraints and
/// resolve them after schema validation has taken place.
#[derive(Debug, Deserialize, Serialize)]
pub struct Constraints {
    #[serde(skip_serializing_if = "is_default")]
    path: InstancePath,

    constraints: HashMap<String, Computed>,
}

impl Constraints {
    pub fn from_path(path: InstancePath) -> Self {
        Self {
            path,
            constraints: HashMap::new(),
        }
    }

    /// Adds a new constraint.
    pub fn add<S, F>(&mut self, name: S, constraint: F)
    where
        S: Into<String>,
        F: Fn(InstancePath) -> Computed,
    {
        self.constraints.insert(name.into(), constraint(self.path.clone()));
    }

    /// Computes the value of all configured constraints based on the given input.
    ///
    /// If a constraint has no computed value, it will not be included in the output. This is the
    /// functional equivalent of an expression returning `None` when the return type is `Option<T>`.
    pub fn compute(&self, input: &Value) -> HashMap<String, Value> {
        self.constraints
            .iter()
            .filter_map(|(k, v)| v.compute(input).map(|v| (k.to_string(), v)))
            .collect()
    }
}

/// A computed constraint.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "op", content = "data")]
pub enum Computed {
    /// A fixed value.
    ///
    /// Generally not applicable as the top-level computed value for a constraint, but more common
    /// for specifying the value of a conditional/optional constraint.
    Fixed(Value),

    /// Flattens array-of-arrays into a single array.
    ///
    /// If the computed value is an array, any array items in the value will be flattened into the
    /// value itself.
    ///
    /// Flattening is not recursive/nested.
    Flatten(Box<Computed>),

    /// An array of computed values.
    ///
    /// Any computed value that returns no value is excluded from the vector of computed values.
    Array(Vec<Computed>),

    /// A value that depends on the result of a conditional.
    ///
    /// If the given conditional is true, the value is computed and returned. Otherwise, no value is returned.
    Optional {
        #[serde(rename = "cond")]
        condition: Condition,
        value: Box<Computed>,
    },

    /// A value that is derived from the input itself.
    ///
    /// The target specifies what portion of the input to derive from, while the operation describes
    /// how to interpret the value: derive it as-is, or slightly transform it, and so on.
    Derived {
        target: InstancePath,

        #[serde(rename = "from")]
        operation: DeriveOp,
    },

    /// A value that is retrieved from a lookup table.
    ///
    /// The key used to lookup the value is retrieved by computing the value of `key`. If the
    /// computed key is anything other than a string, no value is returned.
    ///
    /// If the computed key exists in the lookup table, the computed value of the entry is returned.
    /// Otherwise, no value is returned.
    Lookup {
        key: Box<Computed>,
        table: HashMap<String, Computed>,
    }
}

impl Computed {
    /// Creates a fixed value.
    pub fn fixed<T>(item: T) -> Self
    where
        T: Into<Value>,
    {
        Self::Fixed(item.into())
    }

    /// Creates a flattened value.
    pub fn flatten(value: Computed) -> Self {
        Self::Flatten(Box::new(value))
    }

    /// Creates an array of computed values.
    pub fn array<I, T>(items: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Computed>,
    {
        Self::Array(items.into_iter().map(Into::into).collect())
    }

    /// Creates an optional computed value based on the given `condition`.
    pub fn optional<V>(condition: Condition, value: V) -> Self
    where
        V: Into<Computed>,
    {
        Self::Optional {
            condition,
            value: Box::new(value.into()),
        }
    }

    /// Creates a derived value based on the given `target`.
    pub fn derived<T>(target: T, operation: DeriveOp) -> Self
    where
        T: Into<InstancePath>,
    {
        Self::Derived { target: target.into(), operation }
    }

    /// Creates a lookup table value based on the given `key` and `table`.
    pub fn lookup<T, K, V>(key: Computed, table: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<Computed>,
    {
        Self::Lookup {
            key: Box::new(key),
            table: table.into_iter().map(|(k, v)| (k.into(), v.into())).collect(),
        }
    }

    /// Computes the output of this value.
    ///
    /// If the computed value is null, `None` is returned.
    pub fn compute(&self, input: &Value) -> Option<Value> {
        match self {
            Computed::Fixed(v) => Some(v.clone()).and_then(nonnull_or_none),
            Computed::Flatten(value) => flatten_value(value),
            Computed::Optional { condition, value } => condition
                .check(input)
                .then_some(value.compute(input))
                .flatten(),
            Computed::Array(items) => {
                Some(items.into_iter().filter_map(|c| c.compute(input)).collect())
            }
            Computed::Derived { target, operation } => target.lookup(input)
                .and_then(|value| operation.derive(value)),
            Computed::Lookup { key, table } => key.compute(input)
                .and_then(|v| v.as_str().and_then(|key| table.get(key)))
                .and_then(|v| v.compute(input)),
        }
    }
}

fn flatten_value(value: &Computed) -> Option<Value> {
    todo!()
}

impl<T> From<T> for Computed
where
    T: Into<Value>,
{
    fn from(value: T) -> Self {
        Computed::Fixed(value.into())
    }
}

/// A conditional check.
///
/// Conditions are used to evaluate the given input and then choose a particular result path to
/// further evaluate/compute, such as only including a value if a field equals a known value, or
/// simulating if/else logic.
///
/// Conditions are simplistic, supporting a fixed target path in the input, a comparison operator,
/// and a single operand.
#[derive(Debug, Deserialize, Serialize)]
pub struct Condition {
    target: InstancePath,
    operator: Operator,
    operand: Value,
}

impl Condition {
    /// Creates an equality condition for the given `target`.
    ///
    /// This condition succeeds if the value at the target path in the given input is equal to the `operand`.
    pub fn eq<T, O>(target: T, operand: O) -> Self
    where
        T: Into<InstancePath>,
        O: Into<Value>,
    {
        Self {
            target: target.into(),
            operator: Operator::Eq,
            operand: operand.into(),
        }
    }

    /// Creates an inequality condition for the given `target`.
    ///
    /// This condition succeeds if the value at the target path in the given input is not equal to
    /// the `operand`.
    pub fn ne<T, O>(target: T, operand: O) -> Self
    where
        T: Into<InstancePath>,
        O: Into<Value>,
    {
        Self {
            target: target.into(),
            operator: Operator::Ne,
            operand: operand.into(),
        }
    }

    /// Checks the condition against the given `input`.
    ///
    /// If the condition passes, `true` is returned. Otherwise, `false`.
    pub fn check(&self, input: &Value) -> bool {
        let target_value = self.target.lookup(input).unwrap_or(&Value::Null);
        match self.operator {
            Operator::Eq => target_value == &self.operand,
            Operator::Ne => target_value != &self.operand,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
/// Comparison operators.
pub enum Operator {
    /// Compares two values for equality.
    Eq,

    /// Compares two values for inequality.
    Ne,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeriveOp {
    /// Uses the raw value at the target path.
    Raw,

    /// Takes the keys of an object at the target path.
    ///
    /// If the value at the target path is not an object, the derived value is null instead.
    Keys,

    /// Takes the values of an object, or array, at the target path.
    ///
    /// If the value at the target path is not an object or array, the derived value is null instead.
    Values,
}

impl DeriveOp {
    fn derive(&self, input: &Value) -> Option<Value> {
        match self {
            DeriveOp::Raw => Some(input.clone()),
            DeriveOp::Keys => match input {
                Value::Object(map) => Some(Value::Array(
                    map.keys().map(|s| s.as_str().into()).collect(),
                )),
                _ => None,
            },
            DeriveOp::Values => match input {
                Value::Object(map) => Some(Value::Array(map.values().cloned().collect())),
                array @ Value::Array(_) => Some(array.clone()),
                _ => None,
            },
        }
    }
}

fn nonnull_or_none(value: Value) -> Option<Value> {
    match value {
        Value::Null => None,
        v => Some(v),
    }
}

fn is_default<T: Default + Eq>(value: &T) -> bool {
    let default = T::default();
    value == &default
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn datadog_agent_source() {
        let path = InstancePath::rooted();
        let mut constraints = Constraints::from_path(path);
        constraints.add("outputs", |path| {
            Computed::optional(
                Condition::eq(path.push("multiple_outputs"), true),
                Computed::array([
                    Computed::optional(Condition::ne(path.push("disable_logs"), true), "logs"),
                    Computed::optional(Condition::ne(path.push("disable_metrics"), true), "metrics"),
                    Computed::optional(Condition::ne(path.push("disable_traces"), true), "traces"),
                ]),
            )
        });

        let defaults = json!({});
        println!("with defaults: {:?}", constraints.compute(&defaults));

        let multiple_outputs = json!({
            "multiple_outputs": true
        });
        println!(
            "with multiple_outputs=true: {:?}",
            constraints.compute(&multiple_outputs)
        );

        let multiple_outputs_no_traces = json!({
            "multiple_outputs": true,
            "disable_traces": true
        });
        println!(
            "with multiple_outputs=true, disable_traces=true: {:?}",
            constraints.compute(&multiple_outputs_no_traces)
        );
    }

    #[test]
    fn route_transform() {
        let path = InstancePath::rooted();
        let mut constraints = Constraints::from_path(path);
        constraints.add("outputs", |path| Computed::derived(path.push("route"), DeriveOp::Keys));

        let no_routes = json!({});
        println!("with no routes: {:?}", constraints.compute(&no_routes));

        let some_routes = json!({
            "route": {
                "route1": {},
                "route2": {}
            }
        });
        println!("with some routes: {:?}", constraints.compute(&some_routes));

        println!("serialized: {}", serde_json::to_string(&constraints).unwrap());
    }

    #[test]
    fn inputs_encoding_dependent() {
        let path = InstancePath::rooted();
        let mut constraints = Constraints::from_path(path);
        constraints.add("inputs", |path| Computed::lookup(
            Computed::derived(path.push(&["encoding", "codec"][..]), DeriveOp::Raw),
            [
                ("json", Computed::array(["logs", "metrics", "traces"])),
                ("text", Computed::array(["logs", "traces"])),
            ]
        ));

        let json_codec = json!({
            "encoding": {
                "codec": "json"
            }
        });
        println!("with json codec: {:?}", constraints.compute(&json_codec));

        let text_codec = json!({
            "encoding": {
                "codec": "text"
            }
        });
        println!("with text codec: {:?}", constraints.compute(&text_codec));
    }
}
