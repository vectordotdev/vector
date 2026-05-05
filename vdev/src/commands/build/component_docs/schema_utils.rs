use super::{SchemaContext, get_schema_metadata};
use anyhow::Result;
use serde_json::{Map, Value};

/// Render a JSON Schema `const` scalar (string, number, or bool) as a CUE enum
/// key. CUE enum keys are strings, so non-string scalars are stringified the
/// same way `serde_json` would print them. Non-scalar values return `None`.
fn scalar_const_key(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

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

        let resolved_schema = self.resolve_schema_by_name(schema_name)?;

        let unwrapped_obj = resolved_schema
            .pointer("/type/object/options")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Configuration types must always resolve to an object schema; '{schema_name}' did not. Resolved: {resolved_schema}"
                )
            })?;

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
        if let Some(prop) = schema.get("properties").and_then(|p| p.get(property_name)) {
            return Some(prop);
        }

        // Walk oneOf/anyOf/allOf and collect every subschema's matching property.
        // We can only confidently apply a default through one of these branches if
        // they all describe the same shape, so compare reduced forms and bail if
        // they diverge.
        let mut matches: Vec<&'a Value> = Vec::new();
        let mut unvisited: Vec<&'a Value> = Vec::new();
        for key in &["oneOf", "anyOf", "allOf"] {
            if let Some(Value::Array(arr)) = schema.get(*key) {
                unvisited.extend(arr.iter());
            }
        }

        while let Some(sub) = unvisited.pop() {
            if let Some(prop) = sub.get("properties").and_then(|p| p.get(property_name)) {
                matches.push(prop);
                continue;
            }
            for key in &["oneOf", "anyOf", "allOf"] {
                if let Some(Value::Array(arr)) = sub.get(*key) {
                    unvisited.extend(arr.iter());
                }
            }
        }

        let first = matches.first()?;
        let reduced_first = self.get_reduced_schema(first);
        for other in matches.iter().skip(1) {
            if self.get_reduced_schema(other) != reduced_first {
                return None;
            }
        }
        Some(first)
    }

    pub fn apply_schema_default_value(
        &self,
        source_schema: &Value,
        resolved_schema: &mut Value,
    ) -> Result<()> {
        debug!("Applying schema default values.");

        let default_value = match source_schema.get("default") {
            Some(v) if !v.is_null() => v.clone(),
            _ => return Ok(()),
        };

        let default_value_type = self.get_docs_type_for_value(Some(source_schema), &default_value);

        // The resolved schema must declare a type definition matching the default
        // value's type. Anything else is a schema generation bug, so surface it
        // loudly rather than silently dropping the default.
        if resolved_schema
            .pointer(&format!("/type/{default_value_type}"))
            .is_none()
        {
            anyhow::bail!(
                "Schema has default value declared that does not match type of resolved schema:\n\
                 Source schema: {}\n\
                 Default value: {} (type: {})\n\
                 Resolved schema: {}",
                serde_json::to_string_pretty(source_schema)?,
                serde_json::to_string_pretty(&default_value)?,
                default_value_type,
                serde_json::to_string_pretty(resolved_schema)?,
            );
        }

        if default_value_type == "object" {
            let Value::Object(def_obj) = default_value else {
                anyhow::bail!("Default value typed 'object' was not a JSON object");
            };
            let props = resolved_schema
                .pointer_mut("/type/object/options")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| {
                    anyhow::anyhow!("Resolved object schema is missing /type/object/options")
                })?;

            for (prop_name, prop_default_value) in def_obj {
                if prop_default_value.is_null() {
                    continue;
                }
                let Some(resolved_prop) = props.get_mut(&prop_name) else {
                    continue;
                };

                if let Some(source_prop) =
                    self.find_nested_object_property_schema(source_schema, &prop_name)
                {
                    let mut source_with_default = source_prop.clone();
                    source_with_default
                        .as_object_mut()
                        .unwrap()
                        .insert("default".to_string(), prop_default_value);
                    self.apply_schema_default_value(&source_with_default, resolved_prop)?;
                } else {
                    let value_type = self.get_docs_type_for_value(None, &prop_default_value);
                    if let Some(Value::Object(type_obj)) = resolved_prop.get_mut("type")
                        && let Some(Value::Object(type_def)) = type_obj.get_mut(value_type)
                    {
                        type_def.insert("default".to_string(), prop_default_value);
                    }
                }
                resolved_prop
                    .as_object_mut()
                    .unwrap()
                    .insert("required".to_string(), Value::Bool(false));
            }
        } else {
            let type_def = resolved_schema
                .pointer_mut(&format!("/type/{default_value_type}"))
                .and_then(Value::as_object_mut)
                .expect("/type/{default_value_type} existence verified above");
            type_def.insert("default".to_string(), default_value);
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

        let has_self_default_value = property_schema.get("default").is_some_and(|v| !v.is_null());
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

    pub fn reconcile_resolved_schema(resolved: &mut Value) {
        let Some(type_obj) = resolved.get("type").and_then(Value::as_object) else {
            return;
        };

        if let Some(options) = type_obj
            .get("object")
            .and_then(|o| o.get("options"))
            .and_then(Value::as_object)
        {
            let property_keys: Vec<String> = options.keys().cloned().collect();
            for key in property_keys {
                if let Some(prop) = resolved
                    .pointer_mut(&format!("/type/object/options/{key}"))
                    .filter(|v| v.is_object())
                {
                    Self::reconcile_resolved_schema(prop);
                }
            }
            return;
        }

        let is_required = resolved
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if is_required {
            let type_field_keys: Vec<String> = type_obj.keys().cloned().collect();
            for type_field in &type_field_keys {
                let pointer = format!("/type/{type_field}");
                if let Some(Value::Object(field)) = resolved.pointer_mut(&pointer)
                    && let Some(Value::Null) = field.get("default")
                {
                    field.shift_remove("default");
                }
            }
        }

        let schema_description = resolved
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned);

        let type_field_keys: Vec<String> = resolved
            .get("type")
            .and_then(Value::as_object)
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();

        for type_field in &type_field_keys {
            let const_pointer = format!("/type/{type_field}/const");
            let Some(const_value) = resolved.pointer(&const_pointer).cloned() else {
                continue;
            };

            let entries = match &const_value {
                Value::Array(items) => items
                    .iter()
                    .filter_map(|item| {
                        let key = scalar_const_key(item.get("value")?)?;
                        let desc = item
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        Some((key, desc))
                    })
                    .collect::<Vec<_>>(),
                Value::Object(single) => {
                    let Some(key) = single.get("value").and_then(scalar_const_key) else {
                        continue;
                    };
                    let desc = single
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| schema_description.clone())
                        .unwrap_or_default();
                    vec![(key, desc)]
                }
                _ => continue,
            };

            let mut enum_map = Map::new();
            for (key, desc) in entries {
                enum_map.insert(key, Value::String(desc));
            }

            if let Some(Value::Object(field)) = resolved.pointer_mut(&format!("/type/{type_field}"))
            {
                field.shift_remove("const");
                field.insert("enum".to_string(), Value::Object(enum_map));
            }
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

    pub fn get_docs_type_for_value(&self, schema: Option<&Value>, value: &Value) -> &'static str {
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
