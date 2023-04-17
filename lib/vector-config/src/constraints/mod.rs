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
    pub fn add<S>(&mut self, name: S, constraint: Computed)
    where
        S: Into<String>,
    {
        self.constraint_map.insert(name.into(), constraint);
    }

    pub fn compute(&self, input: &Value) -> HashMap<String, Value> {
        self.constraint_map.iter()
            .filter_map(|(k, v)| v.compute(input).map(|v| (k.to_string(), v)))
            .collect()
    }
}

pub enum Computed {
    Fixed(Value),

    Optional { condition: Condition, value: Box<Computed> },

    Array(Vec<Computed>),

    //IfElse { if_condition: Condition, if_value: Box<Computed>, else_value: Box<Computed> },
}

impl Computed {
    /*pub fn if_else<IV, EV>(if_condition: Condition, if_value: IV, else_value: EV) -> Self
    where
        IV: Into<Computed>,
        EV: Into<Computed>,
    {
        Self::IfElse { if_condition, if_value: Box::new(if_value.into()), else_value: Box::new(else_value.into()) }
    }*/

    pub fn array<I, T>(items: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Computed>,
    {
        Self::Array(items.into_iter().map(Into::into).collect())
    }

    pub fn optional<V>(condition: Condition, value: V) -> Self
    where
        V: Into<Computed>,
    {
        Self::Optional { condition, value: Box::new(value.into()) }
    }

    pub fn compute(&self, input: &Value) -> Option<Value> {
        match self {
            Computed::Fixed(v) => Some(v.clone()).and_then(nonnull_or_none),
            Computed::Optional { condition, value } => condition.check(input).then_some(value.compute(input)).flatten(),
            Computed::Array(items) => Some(items.into_iter().filter_map(|c| c.compute(input)).collect()),
            /*Computed::IfElse { if_condition, if_value, else_value } => {
                if_condition.check(input)
                    .then(|| if_value.compute(input))
                    .flatten()
                    .or_else(|| else_value.compute(input))
            }*/
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

pub struct Condition {
    target: &'static str,
    operator: Operator,
    operand: Value,
}

impl Condition {
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

    pub fn check(&self, input: &Value) -> bool {
        let target_value = input.pointer(self.target).unwrap_or(&Value::Null);
        match self.operator {
            Operator::Eq => target_value == &self.operand,
            Operator::Ne => target_value != &self.operand,
        }
    }
}

pub enum Operator {
    Eq,
    Ne,
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
    fn connectability_datadog_agent_outputs() {
        let mut constraints = Constraints::default();
        constraints.add("outputs", Computed::optional(
            Condition::eq("/multiple_outputs", true),
            Computed::array([
                Computed::optional(Condition::ne("/disable_logs", true), "logs"),
                Computed::optional(Condition::ne("/disable_metrics", true), "metrics"),
                Computed::optional(Condition::ne("/disable_traces", true), "traces"),
            ]),
        ));

        let defaults = json!({});
        println!("with defaults: {:?}", constraints.compute(&defaults));

        let multiple_outputs = json!({
            "multiple_outputs": true
        });
        println!("with multiple_outputs=true: {:?}", constraints.compute(&multiple_outputs));

        let multiple_outputs_no_traces = json!({
            "multiple_outputs": true,
            "disable_traces": true
        });
        println!("with multiple_outputs=true, disable_traces=true: {:?}", constraints.compute(&multiple_outputs_no_traces));
    }
}
