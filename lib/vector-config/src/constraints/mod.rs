#![allow(warnings)]

use std::collections::HashMap;

use serde_json::Value;

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
#[derive(Default)]
pub struct Constraints {
    constraint_map: HashMap<String, Computed>,
}

impl Constraints {
    /// Adds a new constraint.
    pub fn add<S>(&mut self, name: S, constraint: Computed)
    where
        S: Into<String>,
    {
        self.constraint_map.insert(name.into(), constraint);
    }

    /// Computes the value of all configured constraints based on the given input.
    ///
    /// If a constraint has no computed value, it will not be included in the output. This is the
    /// functional equivalent of an expression returning `None` when the return type is `Option<T>`.
    pub fn compute(&self, input: &Value) -> HashMap<String, Value> {
        self.constraint_map
            .iter()
            .filter_map(|(k, v)| v.compute(input).map(|v| (k.to_string(), v)))
            .collect()
    }
}

/// A computed constraint.
pub enum Computed {
    /// A fixed value.
    ///
    /// Generally not applicable as the top-level computed value for a constraint, but more common
    /// for specifying the value of a conditional/optional constraint.
    Fixed(Value),

    /// An array of computed values.
    ///
    /// Any computed value that returns no value is excluded from the vector of computed values.
    Array(Vec<Computed>),

    /// A value that depends on the result of a conditional.
    ///
    /// If the given conditional is true, the value is computed and returned. Otherwise, no value is returned.
    Optional {
        condition: Condition,
        value: Box<Computed>,
    },

    /// A value that is derived from the input itself.
    ///
    /// A target path specifies what portion of the input to derive from, while the operation
    /// describes how to interpret the value: derive it as-is, or slightly transform it, and so on.
    Derived {
        target: &'static str,
        operation: DeriveOp,
    },
}

impl Computed {
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
    pub fn derived(target: &'static str, operation: DeriveOp) -> Self {
        Self::Derived { target, operation }
    }

    /// Computes the output of this value.
    ///
    /// If the computed value is null, `None` is returned.
    pub fn compute(&self, input: &Value) -> Option<Value> {
        match self {
            Computed::Fixed(v) => Some(v.clone()).and_then(nonnull_or_none),
            Computed::Optional { condition, value } => condition
                .check(input)
                .then_some(value.compute(input))
                .flatten(),
            Computed::Array(items) => {
                Some(items.into_iter().filter_map(|c| c.compute(input)).collect())
            }
            Computed::Derived { target, operation } => input
                .pointer(target)
                .and_then(|value| operation.derive(value)),
        }
    }
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
pub struct Condition {
    target: &'static str,
    operator: Operator,
    operand: Value,
}

impl Condition {
    /// Creates an equality condition for the given `target`.
    ///
    /// This condition succeeds if the value at the target path in the given input is equal to the `operand`.
    pub fn eq<O>(target: &'static str, operand: O) -> Self
    where
        O: Into<Value>,
    {
        Self {
            target,
            operator: Operator::Eq,
            operand: operand.into(),
        }
    }

    /// Creates an inequality condition for the given `target`.
    ///
    /// This condition succeeds if the value at the target path in the given input is not equal to
    /// the `operand`.
    pub fn ne<O>(target: &'static str, operand: O) -> Self
    where
        O: Into<Value>,
    {
        Self {
            target,
            operator: Operator::Ne,
            operand: operand.into(),
        }
    }

    /// Checks the condition against the given `input`.
    ///
    /// If the condition passes, `true` is returned. Otherwise, `false`.
    pub fn check(&self, input: &Value) -> bool {
        let target_value = input.pointer(self.target).unwrap_or(&Value::Null);
        match self.operator {
            Operator::Eq => target_value == &self.operand,
            Operator::Ne => target_value != &self.operand,
        }
    }
}

/// Comparison operators.
pub enum Operator {
    /// Compares two values for equality.
    Eq,

    /// Compares two values for inequality.
    Ne,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn datadog_agent_source() {
        let mut constraints = Constraints::default();
        constraints.add(
            "outputs",
            Computed::optional(
                Condition::eq("/multiple_outputs", true),
                Computed::array([
                    Computed::optional(Condition::ne("/disable_logs", true), "logs"),
                    Computed::optional(Condition::ne("/disable_metrics", true), "metrics"),
                    Computed::optional(Condition::ne("/disable_traces", true), "traces"),
                ]),
            ),
        );

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
        let mut constraints = Constraints::default();
        constraints.add("outputs", Computed::derived("/route", DeriveOp::Keys));

        let no_routes = json!({});
        println!("with no routes: {:?}", constraints.compute(&no_routes));

        let some_routes = json!({
            "route": {
                "route1": {},
                "route2": {}
            }
        });
        println!("with some routes: {:?}", constraints.compute(&some_routes));
    }
}
