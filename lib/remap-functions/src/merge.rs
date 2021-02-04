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
                accepts: |v| matches!(v, Value::Map(_)),
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
        let to = arguments.required("to")?.boxed();
        let from = arguments.required("from")?.boxed();
        let deep = arguments.optional("deep").map(Expr::boxed);

        Ok(Box::new(MergeFn { to, from, deep }))
    }
}

#[derive(Debug, Clone)]
pub struct MergeFn {
    to: Box<dyn Expression>,
    from: Box<dyn Expression>,
    deep: Option<Box<dyn Expression>>,
}

impl Expression for MergeFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let mut to_value = self.to.execute(state, object)?.try_map()?;
        let from_value = self.from.execute(state, object)?.try_map()?;
        let deep = match &self.deep {
            None => false,
            Some(deep) => deep.execute(state, object)?.try_boolean()?,
        };

        merge_maps(&mut to_value, &from_value, deep);

        Ok(to_value.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let from = self.from.type_def(state);

        let to = self.to.type_def(state);
        let from_inner = from.inner_type_def.clone();

        let mut type_def = from
            .fallible_unless(value::Kind::Map)
            .merge(to.fallible_unless(value::Kind::Map))
            .merge_optional(
                self.deep
                    .as_ref()
                    .map(|deep| deep.type_def(state).fallible_unless(value::Kind::Boolean)),
            )
            .with_constraint(value::Kind::Map);

        type_def.inner_type_def = merge_inner_type_defs(type_def.inner_type_def, from_inner);
        type_def
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

/// Merges the inner type defs of the two maps.
/// The merge has to be a shallow one, since we don't necessarily know the value of the `deep`
/// parameter at compile time.
fn merge_inner_type_defs(
    to: Option<InnerTypeDef>,
    from: Option<InnerTypeDef>,
) -> Option<InnerTypeDef> {
    match (to, from) {
        (Some(InnerTypeDef::Map(map1)), Some(InnerTypeDef::Map(map2)))
        | (Some(InnerTypeDef::Both { map: map1, .. }), Some(InnerTypeDef::Map(map2)))
        | (Some(InnerTypeDef::Map(map1)), Some(InnerTypeDef::Both { map: map2, .. }))
        | (
            Some(InnerTypeDef::Both { map: map1, .. }),
            Some(InnerTypeDef::Both { map: map2, .. }),
        ) => {
            // Any combinations of maps, we can merge these and use as the resulting inner type def.
            Some(InnerTypeDef::Map(
                map1.into_iter().chain(map2.into_iter()).collect(),
            ))
        }
        (None, Some(InnerTypeDef::Map(map))) | (None, Some(InnerTypeDef::Both { map, .. })) => {
            // Otherwise, if the to inner_type_def is not known the from type_def will override
            // any fields within the to, so we can take this on.
            // The same does not hold if we don't know the type def of the from, these fields could
            // override any of the known fields in the to.
            Some(InnerTypeDef::Map(map))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use value::Kind;

    test_type_def![

        value_non_maps {
            expr: |_| MergeFn {
                to: array!["ook"].boxed(),
                from: array!["ook"].boxed(),
                deep: None
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
                inner_type_def: None
            },
        }

        value_maps {
            expr: |_| MergeFn {
                to: remap::map![ "ook" : 2,
                                 "nork" : "oog",
                                 "both" : 3
                ].boxed(),
                from: remap::map![ "ook" : 4,
                                   "both" : "nerg"
                ].boxed(),
                deep: None
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Map,
                inner_type_def: Some(inner_type_def! ({ "ook": Kind::Integer,
                                                         "nork": Kind::Bytes,
                                                         "both": Kind::Bytes
                }))
            },
        }
    ];

    test_function! [
        merge => Merge;

        simple {
            args: func_args![ to: btreemap! { "key1" => "val1" },
                              from: btreemap! { "key2"=> "val2" }
            ],
            want: Ok(btreemap! {
                "key1" => "val1",
                "key2" => "val2",
            })
        }

        shallow {
            args: func_args![
                to: btreemap! {
                    "key1" => "val1",
                    "child" => btreemap! { "grandchild1" => "val1" },
                },
                from: btreemap! {
                    "key2" => "val2",
                    "child" => btreemap! { "grandchild2" => "val2" },
                }
            ],
            want: Ok(btreemap! {
                "key1" => "val1",
                "key2" => "val2",
                "child" => btreemap! { "grandchild2" => "val2" },
            })
        }

        deep {
            args: func_args![
                to: btreemap! {
                    "key1" => "val1",
                    "child" => btreemap! { "grandchild1" => "val1" },
                },
                from: btreemap! {
                    "key2" => "val2",
                    "child" => btreemap! { "grandchild2" => "val2" },
                },
                deep: true
            ],
            want: Ok(btreemap!{
                "key1" => "val1",
                "key2" => "val2",
                "child" => btreemap! {
                    "grandchild1" => "val1",
                    "grandchild2" => "val2",
                },
            })
        }
    ];
}
