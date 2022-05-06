use ::value::kind::Unknown;
use vrl::prelude::*;

use vrl::prelude::TypeDef as VrlTypeDef;

fn type_def(type_def: &VrlTypeDef) -> Resolved {
    let mut tree = BTreeMap::new();
    insert_if_true(&mut tree, "fallible", type_def.is_fallible());
    insert_kind(&mut tree, type_def.kind(), true);
    Ok(Value::Object(tree))
}

fn insert_kind(tree: &mut BTreeMap<String, Value>, kind: &Kind, show_unknown: bool) {
    if kind.is_any() {
        insert_if_true(tree, "any", true);
    } else {
        insert_if_true(tree, "bytes", kind.contains_bytes());
        insert_if_true(tree, "integer", kind.contains_integer());
        insert_if_true(tree, "float", kind.contains_float());
        insert_if_true(tree, "boolean", kind.contains_boolean());
        insert_if_true(tree, "timestamp", kind.contains_timestamp());
        insert_if_true(tree, "regex", kind.contains_regex());
        insert_if_true(tree, "null", kind.contains_null());

        if let Some(fields) = &kind.object {
            let mut object_tree = BTreeMap::new();
            for (field, field_kind) in fields.known() {
                let mut field_tree = BTreeMap::new();
                insert_kind(&mut field_tree, field_kind, show_unknown);
                object_tree.insert(field.name.clone(), Value::Object(field_tree));
            }
            tree.insert("object".to_owned(), Value::Object(object_tree));
            if show_unknown {
                insert_unknown(tree, fields.unknown(), "object");
            }
        }

        if let Some(indices) = &kind.array {
            let mut array_tree = BTreeMap::new();
            for (index, index_kind) in indices.known() {
                let mut index_tree = BTreeMap::new();
                insert_kind(&mut index_tree, index_kind, show_unknown);
                array_tree.insert(index.to_string(), Value::Object(index_tree));
            }
            tree.insert("array".to_owned(), Value::Object(array_tree));
            if show_unknown {
                insert_unknown(tree, indices.unknown(), "array");
            }
        }
    }
}

fn insert_unknown(tree: &mut BTreeMap<String, Value>, unknown: Option<&Unknown>, prefix: &str) {
    if let Some(unknown) = unknown {
        let mut unknown_tree = BTreeMap::new();
        insert_kind(&mut unknown_tree, unknown.to_kind().as_ref(), false);
        if unknown.is_exact() {
            tree.insert(
                format!("{}_unknown_exact", prefix),
                Value::Object(unknown_tree),
            );
        } else {
            tree.insert(
                format!("{}_unknown_infinite", prefix),
                Value::Object(unknown_tree),
            );
        }
    }
}

fn insert_if_true(tree: &mut BTreeMap<String, Value>, key: &str, value: bool) {
    if value {
        tree.insert(key.to_owned(), Value::Boolean(true));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TypeDef;

impl Function for TypeDef {
    fn identifier(&self) -> &'static str {
        "type_def"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[]
    }

    fn compile(
        &self,
        state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        Ok(Box::new(TypeDefFn {
            type_def: value.type_def((&*state.0, &*state.1)),
        }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, _args: &mut VmArgumentList) -> Resolved {
        Ok(Value::from(
            "Unimplemented. Switch to the AST runtime to use this function.",
        ))
    }
}

#[derive(Debug, Clone)]
struct TypeDefFn {
    type_def: VrlTypeDef,
}

impl Expression for TypeDefFn {
    fn resolve(&self, _ctx: &mut Context) -> Resolved {
        type_def(&self.type_def.clone())
    }

    fn type_def(&self, _state: (&state::LocalEnv, &state::ExternalEnv)) -> VrlTypeDef {
        VrlTypeDef::any().infallible()
    }
}
