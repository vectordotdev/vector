use std::collections::BTreeMap;

use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Merge;

impl Function for Merge {
    fn identifier(&self) -> &'static str {
        "merge"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "to",
                accepts: |_| true,
                required: false,
            },
            Parameter {
                keyword: "from",
                accepts: |v| matches!(v, Value::Map(_)),
                required: true,
            },
            Parameter {
                keyword: "deep",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let to = arguments.required_path("to")?;
        let from = arguments.required("from")?.boxed();
        let deep = arguments.optional("deep").map(Expr::boxed);

        Ok(Box::new(MergeFn { to, from, deep }))
    }
}

#[derive(Debug, Clone)]
pub struct MergeFn {
    to: Path,
    from: Box<dyn Expression>,
    deep: Option<Box<dyn Expression>>,
}

impl MergeFn {
    #[cfg(test)]
    pub fn new(to: Path, from: Box<dyn Expression>, deep: Option<Box<dyn Expression>>) -> Self {
        Self { to, from, deep }
    }
}

impl Expression for MergeFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let from_value = self.from.execute(state, object)?.try_map()?;
        let deep = match &self.deep {
            None => false,
            Some(deep) => deep.execute(state, object)?.try_boolean()?,
        };

        match object.get(&self.to.as_ref())? {
            Some(Value::Map(mut map1)) => {
                merge_maps(&mut map1, &from_value, deep);
                object.insert(&self.to.as_ref(), Value::Map(map1))?;
                Ok(Value::Null)
            }
            _ => Err("parameters passed to merge are non-map values".into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.from
            .type_def(state)
            .fallible_unless(value::Kind::Map)
            .merge_optional(
                self.deep
                    .as_ref()
                    .map(|deep| deep.type_def(state).fallible_unless(value::Kind::Boolean)),
            )
            .with_constraint(value::Kind::Null)
    }
}

/// Merges two BTreeMaps of Symbol’s value as variable is void: Values.
/// The second map is merged into the first one.
///
/// If Symbol’s value as variable is void: deep is true, only the top level values are merged in. If both maps contain a field
/// with the same name, the field from the first is overwritten with the field from the second.
///
/// If Symbol’s value as variable is void: deep is false, should both maps contain a field with the same name, and both those
/// fields are also maps, the function will recurse and will merge the child fields from the second
/// into the child fields from the first.
///
/// Note, this does recurse, so there is the theoretical possibility that it could blow up the
/// stack. From quick tests on a sample project I was able to merge maps with a depth of 3,500
/// before encountering issues. So I think that is likely to be within acceptable limits.
/// If it becomes a problem, we can unroll this function, but that will come at a cost of extra
/// code complexity.
fn merge_maps<K>(map1: &mut BTreeMap<K, Value>, map2: &BTreeMap<K, Value>, deep: bool)
where
    K: std::cmp::Ord + Clone,
{
    for (key2, value2) in map2.iter() {
        match (deep, map1.get_mut(key2), value2) {
            (true, Some(Value::Map(ref mut child1)), Value::Map(ref child2)) => {
                // We are doing a deep merge and both fields are maps.
                merge_maps(child1, child2, deep);
            }
            _ => {
                map1.insert(key2.clone(), value2.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn merge() {
        let cases = vec![
            (
                map!["foo": Value::Boolean(true),
                     "bar": map!["key2": "val2"]],
                map!["foo": Value::Boolean(true),
                     "bar": map!["key2": "val2"]],
                MergeFn::new(Path::from("foo"), Box::new(Path::from("bar")), None),
                Err("function call error: parameters passed to merge are non-map values".into()),
            ),
            (
                map!["foo": map![ "key1": "val1" ], "bar": map![ "key2": "val2" ]],
                map![
                    "foo":
                        map![ "key1": "val1",
                              "key2": "val2" ],
                    "bar": map![ "key2": "val2" ]
                ],
                MergeFn::new(Path::from("foo"), Box::new(Path::from("bar")), None),
                Ok(Value::Null),
            ),
            (
                map![
                    "parent1":
                        map![ "key1": "val1",
                                       "child": map! [ "grandchild1": "val1" ] ],
                    "parent2":
                        map![ "key2": "val2",
                                       "child": map! [ "grandchild2": "val2" ] ]
                ],
                map![
                    "parent1":
                        map![ "key1": "val1",
                                      "key2": "val2",
                                      "child": map! [ "grandchild2": "val2" ] ],
                    "parent2":
                        map! [ "key2": "val2",
                                       "child": map! [ "grandchild2": "val2" ] ]
                ],
                MergeFn::new(Path::from("parent1"), Box::new(Path::from("parent2")), None),
                Ok(Value::Null),
            ),
            (
                map![
                    "parent1":
                        map![ "key1": "val1",
                              "child": map! [ "grandchild1": "val1" ] ],
                    "parent2":
                        map![ "key2": "val2",
                              "child": map! [ "grandchild2": "val2" ] ]
                ],
                map![
                    "parent1":
                        map![ "key1": "val1",
                              "key2": "val2",
                              "child": map! [ "grandchild1": "val1",
                                              "grandchild2": "val2" ] ],
                    "parent2":
                        map![ "key2": "val2",
                              "child": map! [ "grandchild2": "val2" ] ]
                ],
                MergeFn::new(
                    Path::from("parent1"),
                    Box::new(Path::from("parent2")),
                    Some(Box::new(Literal::from(Value::Boolean(true)))),
                ),
                Ok(Value::Null),
            ),
        ];

        let mut state = state::Program::default();
        for (input_event, exp_event, func, exp_result) in cases {
            let mut input_event = Value::Map(input_event);
            let exp_event = Value::Map(exp_event);

            let got = func
                .execute(&mut state, &mut input_event)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(input_event, exp_event);
            assert_eq!(got, exp_result);
        }
    }
}
