use super::{SchemaContext, docs_type_str, get_schema_metadata};
use anyhow::Result;
use serde_json::{Map, Value};

impl SchemaContext {
    pub fn get_rendered_description_from_schema(&self, schema: &Value) -> String {
        let raw_description = schema
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let raw_title = schema.get("title").and_then(|v| v.as_str()).unwrap_or("");

        let description = if raw_title.is_empty() {
            raw_description.to_string()
        } else {
            format!("{raw_title}\n\n{raw_description}")
        };
        description.trim().to_string()
    }

    pub fn unwrap_resolved_schema(
        &mut self,
        schema_name: &str,
        friendly_name: &str,
    ) -> Result<Map<String, Value>> {
        info!("[*] Resolving schema definition for {}...", friendly_name);

        let mut resolved_schema = self.resolve_schema_by_name(schema_name)?;

        let unwrapped = resolved_schema.pointer_mut("/type/object/options");
        if unwrapped.is_none() {
            error!("Configuration types must always resolve to an object schema.");
            std::process::exit(1);
        }

        let unwrapped_obj = unwrapped.unwrap().as_object().unwrap().clone();

        // Recursively sort the entire schema to match Ruby's `sort_hash_nested` logic
        Ok(Self::sort_hash_nested(&unwrapped_obj))
    }

    pub fn fix_grouped_enums_if_numeric(
        &self,
        grouped: &mut indexmap::IndexMap<String, Vec<Value>>,
    ) {
        let mut numeric_vals = Vec::new();
        if let Some(ints) = grouped.shift_remove("integer") {
            numeric_vals.extend(ints);
        }
        if let Some(nums) = grouped.shift_remove("number") {
            numeric_vals.extend(nums);
        }

        if !numeric_vals.is_empty() {
            let is_integer = numeric_vals.iter().all(|v| v.is_i64() || v.is_u64());
            let within_uint = numeric_vals.iter().all(serde_json::Value::is_u64);
            let contains_signed = numeric_vals
                .iter()
                .all(|v| v.is_i64() || v.as_i64().is_some());

            let numeric_type = if !is_integer || (!contains_signed && !within_uint) {
                "float"
            } else if within_uint {
                "uint"
            } else if contains_signed {
                "int"
            } else {
                "float"
            };

            grouped.insert(numeric_type.to_string(), numeric_vals);
        }
    }

    #[allow(clippy::self_only_used_in_recursion)]
    pub fn get_reduced_schema(&self, schema: &Value) -> Value {
        let mut reduced = schema.clone();
        if let Value::Object(ref mut map) = reduced {
            let allowed_properties = [
                "type",
                "const",
                "enum",
                "allOf",
                "oneOf",
                "$ref",
                "items",
                "properties",
            ];
            map.retain(|k, _| allowed_properties.contains(&k.as_str()));

            if let Some(items) = map.get_mut("items") {
                *items = self.get_reduced_schema(items);
            }

            if let Some(Value::Object(properties)) = map.get_mut("properties") {
                for (_, prop) in properties.iter_mut() {
                    *prop = self.get_reduced_schema(prop);
                }
            }

            for key in &["allOf", "oneOf"] {
                if let Some(Value::Array(arr)) = map.get_mut(*key) {
                    for sub in arr.iter_mut() {
                        *sub = self.get_reduced_schema(sub);
                    }
                }
            }
        }
        reduced
    }

    #[allow(clippy::self_only_used_in_recursion)]
    pub fn get_reduced_resolved_schema(&self, schema: &Value) -> Value {
        let mut reduced = schema.clone();
        let allowed_types = [
            "condition",
            "object",
            "array",
            "enum",
            "const",
            "string",
            "bool",
            "float",
            "int",
            "uint",
        ];

        if let Value::Object(ref mut map) = reduced {
            map.retain(|k, _| k == "type");

            if let Some(Value::Object(type_defs)) = map.get_mut("type") {
                type_defs.retain(|k, _| allowed_types.contains(&k.as_str()));

                for (type_name, type_def) in type_defs.iter_mut() {
                    if type_name == "object" {
                        if let Value::Object(def_map) = type_def {
                            def_map.retain(|k, _| k == "options");
                            if let Some(Value::Object(opts)) = def_map.get_mut("options") {
                                for (_, prop) in opts.iter_mut() {
                                    *prop = self.get_reduced_resolved_schema(prop);
                                }
                            }
                        }
                    } else if type_name == "array" {
                        if let Value::Object(def_map) = type_def {
                            def_map.retain(|k, _| k == "items");
                            if let Some(items) = def_map.get_mut("items") {
                                *items = self.get_reduced_resolved_schema(items);
                            }
                        }
                    } else if let Value::Object(def_map) = type_def {
                        def_map.retain(|k, _| allowed_types.contains(&k.as_str()));
                    }
                }
            }
        }
        reduced
    }

    pub fn find_nested_object_property_schema<'a>(
        &self,
        schema: &'a Value,
        property_name: &str,
    ) -> Option<&'a Value> {
        if let Some(prop) = schema
            .get("properties")
            .and_then(|p| p.get(property_name))
        {
            return Some(prop);
        }

        let mut unvisited: Vec<&'a Value> = Vec::new();
        for key in &["oneOf", "anyOf", "allOf"] {
            if let Some(Value::Array(arr)) = schema.get(*key) {
                unvisited.extend(arr.iter());
            }
        }

        while let Some(sub) = unvisited.pop() {
            if let Some(prop) = sub.get("properties").and_then(|p| p.get(property_name)) {
                return Some(prop);
            }
            for key in &["oneOf", "anyOf", "allOf"] {
                if let Some(Value::Array(arr)) = sub.get(*key) {
                    unvisited.extend(arr.iter());
                }
            }
        }

        None
    }

    pub fn apply_schema_default_value(
        &self,
        source_schema: &Value,
        resolved_schema: &mut Value,
    ) -> Result<()> {
        debug!("Applying schema default values.");

        let default_value = match source_schema.get("default") {
            Some(v) if !v.is_null() => v,
            _ => return Ok(()),
        };

        let default_value_type = docs_type_str(default_value);

        if default_value_type == "object" {
            if let Some(resolved_type_field) = resolved_schema.pointer_mut("/type/object")
                && let Value::Object(type_field) = resolved_type_field
                && let Some(Value::Object(props)) = type_field.get_mut("options")
                && let Value::Object(def_obj) = default_value
            {
                for (prop_name, prop_default_value) in def_obj {
                    if prop_default_value.is_null() {
                        continue;
                    }
                    if let Some(resolved_prop) = props.get_mut(prop_name) {
                        let source_property =
                            self.find_nested_object_property_schema(source_schema, prop_name);
                        if let Some(source_prop) = source_property {
                            let mut source_with_default = source_prop.clone();
                            source_with_default
                                .as_object_mut()
                                .unwrap()
                                .insert("default".to_string(), prop_default_value.clone());
                            self.apply_schema_default_value(
                                &source_with_default,
                                resolved_prop,
                            )?;
                        } else {
                            let value_type =
                                self.get_docs_type_for_value(None, prop_default_value);
                            if let Some(Value::Object(type_obj)) = resolved_prop.get_mut("type")
                                && let Some(Value::Object(type_def)) =
                                    type_obj.get_mut(value_type)
                            {
                                type_def.insert(
                                    "default".to_string(),
                                    prop_default_value.clone(),
                                );
                            }
                        }
                        resolved_prop
                            .as_object_mut()
                            .unwrap()
                            .insert("required".to_string(), Value::Bool(false));
                    }
                }
            }
        } else {
            let value_type = self.get_docs_type_for_value(Some(source_schema), default_value);
            if let Some(Value::Object(type_obj)) = resolved_schema.get_mut("type")
                && let Some(Value::Object(type_def)) = type_obj.get_mut(value_type)
            {
                type_def.insert("default".to_string(), default_value.clone());
            }
        }
        Ok(())
    }

    pub fn apply_schema_metadata(&self, source_schema: &Value, resolved_schema: &mut Value) {
        let is_templateable = get_schema_metadata(source_schema, "docs::templateable")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if let Some(Value::Object(types)) = resolved_schema.get_mut("type")
            && let Some(Value::Object(string_def)) = types.get_mut("string")
            && is_templateable
        {
            string_def.insert("syntax".to_string(), Value::String("template".to_string()));
        }

        if let Some(examples) = get_schema_metadata(source_schema, "docs::examples") {
            let mut flattened_examples = match examples {
                Value::Array(arr) => arr.clone(),
                v => vec![v.clone()],
            };

            for ex in &mut flattened_examples {
                if let Value::Object(obj) = ex {
                    let sorted_obj = Self::sort_hash_nested(obj);
                    *ex = Value::Object(sorted_obj);
                }
            }

            if let Some(Value::Object(type_obj)) = resolved_schema.get_mut("type") {
                for (type_name, def) in type_obj.iter_mut() {
                    if let Value::Object(def_map) = def {
                        if type_name == "array" {
                            if let Some(Value::Object(items_obj)) = def_map.get_mut("items")
                                && let Some(Value::Object(subtypes)) = items_obj.get_mut("type")
                            {
                                for (subtype_name, subtype_def) in subtypes.iter_mut() {
                                    if subtype_name != "array"
                                        && let Value::Object(s_def) = subtype_def
                                    {
                                        s_def.insert(
                                            "examples".to_string(),
                                            Value::Array(flattened_examples.clone()),
                                        );
                                    }
                                }
                            }
                        } else {
                            def_map.insert(
                                "examples".to_string(),
                                Value::Array(flattened_examples.clone()),
                            );
                        }
                    }
                }
            }
        }

        if let Some(type_unit) = get_schema_metadata(source_schema, "docs::type_unit") {
            let unit_str = match type_unit {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            if let Some(schema_type) = self.numeric_schema_type(resolved_schema)
                && let Some(Value::Object(types)) = resolved_schema.get_mut("type")
                && let Some(Value::Object(def)) = types.get_mut(schema_type)
            {
                def.insert("unit".to_string(), Value::String(unit_str));
            }
        }

        if let Some(syntax_override) = get_schema_metadata(source_schema, "docs::syntax_override") {
            let syntax_str = match syntax_override {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            if self.resolved_schema_type(resolved_schema) == Some("string")
                && let Some(Value::Object(types)) = resolved_schema.get_mut("type")
                && let Some(Value::Object(string_def)) = types.get_mut("string")
            {
                string_def.insert("syntax".to_string(), Value::String(syntax_str));
            }
        }
    }

    pub fn sort_hash_nested(
        input: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        let mut sorted = serde_json::Map::new();
        let mut keys: Vec<&String> = input.keys().collect();
        keys.sort();
        for key in keys {
            let val = input.get(key).unwrap();
            let new_val = if let Value::Object(obj) = val {
                Value::Object(Self::sort_hash_nested(obj))
            } else {
                val.clone()
            };
            sorted.insert(key.clone(), new_val);
        }
        sorted
    }

    pub fn apply_object_property_fields(
        &self,
        parent_schema: &Value,
        property_schema: &Value,
        property_name: &str,
        property: &mut Value,
    ) {
        let required_properties = parent_schema.get("required").and_then(|r| r.as_array());

        let has_self_default_value = property_schema
            .get("default")
            .is_some_and(|v| !v.is_null());
        let has_parent_default_value = parent_schema
            .get("default")
            .and_then(|d| d.get(property_name))
            .is_some_and(|v| !v.is_null());
        let has_default_value = has_self_default_value || has_parent_default_value;

        let is_required = required_properties
            .is_some_and(|reqs| reqs.contains(&Value::String(property_name.to_string())))
            || property_schema
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);

        property.as_object_mut().unwrap().insert(
            "required".to_string(),
            Value::Bool(is_required && !has_default_value),
        );
    }

    pub fn reconcile_resolved_schema(&self, resolved: &mut Value) {
        if let Some(Value::Object(type_obj)) = resolved.get_mut("type")
            && type_obj.contains_key("object")
            && type_obj.len() > 1
        {
            // Remove map from mixed modes, simplifying
        }
    }

    pub fn numeric_schema_type(&self, resolved_schema: &Value) -> Option<&'static str> {
        let schema_type = self.resolved_schema_type(resolved_schema)?;
        if matches!(schema_type, "uint" | "int" | "float") {
            Some(schema_type)
        } else {
            None
        }
    }

    pub fn resolved_schema_type(&self, resolved_schema: &Value) -> Option<&'static str> {
        if let Some(Value::Object(types)) = resolved_schema.get("type")
            && types.len() == 1
        {
            let type_name = types.keys().next().unwrap();
            return match type_name.as_str() {
                "object" => Some("object"),
                "array" => Some("array"),
                "string" => Some("string"),
                "bool" => Some("bool"),
                "uint" => Some("uint"),
                "int" => Some("int"),
                "float" => Some("float"),
                "condition" => Some("condition"),
                "enum" => Some("enum"),
                "const" => Some("const"),
                "*" => Some("*"),
                _ => None,
            };
        }
        None
    }

    pub fn get_docs_type_for_value(
        &self,
        schema: Option<&Value>,
        value: &Value,
    ) -> &'static str {
        let value_type = super::json_type_str(value);
        if matches!(value_type, "number" | "integer")
            && let Some(s) = schema
            && let Some(numeric_type) =
                get_schema_metadata(s, "docs::numeric_type").and_then(|n| n.as_str())
        {
            return match numeric_type {
                "uint" => "uint",
                "int" => "int",
                "float" => "float",
                _ => super::docs_type_str(value),
            };
        }
        super::docs_type_str(value)
    }
}
