use super::{SchemaContext, get_schema_ref, nested_merge};
use anyhow::{Result, anyhow};
use serde_json::Value;

impl SchemaContext {
    pub fn get_json_schema_instance_type<'a>(&self, schema: &'a Value) -> Option<&'a str> {
        let maybe_type = schema.get("type")?;

        if maybe_type.is_null() || maybe_type.as_str() == Some("null") {
            return None;
        }

        if let Value::Array(arr) = maybe_type {
            let filtered: Vec<_> = arr.iter().filter(|v| v.as_str() != Some("null")).collect();
            if filtered.len() == 1 {
                return filtered[0].as_str();
            }
        } else if let Some(s) = maybe_type.as_str() {
            return Some(s);
        }

        None
    }

    pub fn get_json_schema_type<'a>(&self, schema: &'a Value) -> Option<&'a str> {
        if schema.get("allOf").is_some() {
            Some("all-of")
        } else if schema.get("oneOf").is_some() {
            Some("one-of")
        } else if schema.get("anyOf").is_some() {
            Some("any-of")
        } else if schema.get("type").is_some() {
            self.get_json_schema_instance_type(schema)
        } else if schema.get("const").is_some() {
            Some("const")
        } else if schema.get("enum").is_some() {
            Some("enum")
        } else {
            None
        }
    }

    pub fn get_schema_by_name(&self, schema_name: &str) -> Result<Value> {
        let name = schema_name.replace("#/definitions/", "");
        let def = self
            .root_schema
            .get("definitions")
            .and_then(|defs| defs.get(&name))
            .cloned();

        if let Some(d) = def {
            Ok(d)
        } else {
            Err(anyhow!(
                "Could not find schema definition '{name}' in given schema."
            ))
        }
    }

    pub fn expand_schema_references(&mut self, unexpanded_schema: &Value) -> Result<Value> {
        let mut schema = unexpanded_schema.clone();

        let original_title = schema.get("title").cloned();
        let original_description = schema.get("description").cloned();

        let schema_ref = get_schema_ref(&schema).map(String::from);
        if let Some(r) = schema_ref {
            let expanded_ref = if let Some(cached) = self.expanded_schema_cache.get(&r) {
                cached.clone()
            } else {
                debug!("Expanding top-level schema ref of '{}'...", r);
                let unexpanded = self.get_schema_by_name(&r)?;
                let expanded = self.expand_schema_references(&unexpanded)?;
                self.expanded_schema_cache
                    .insert(r.clone(), expanded.clone());
                expanded
            };

            let obj = schema.as_object_mut().unwrap();
            obj.shift_remove("$ref");

            let mut new_schema = expanded_ref.clone();
            nested_merge(&mut new_schema, &Value::Object(obj.clone()));
            schema = new_schema;
        }

        if let Some(items) = schema.get("items").cloned()
            && items.get("$ref").is_some()
        {
            let expanded_items = self.expand_schema_references(&items)?;
            let items_mut = schema.get_mut("items").unwrap().as_object_mut().unwrap();
            items_mut.shift_remove("$ref");

            let mut new_items = expanded_items;
            nested_merge(&mut new_items, &Value::Object(items_mut.clone()));
            *schema.get_mut("items").unwrap() = new_items;
        }

        if let Some(Value::Object(properties)) = schema.get_mut("properties") {
            for (_, prop_schema) in properties.iter_mut() {
                *prop_schema = self.expand_schema_references(&prop_schema.clone())?;
            }
        }

        for key in &["allOf", "oneOf", "anyOf"] {
            if let Some(Value::Array(arr)) = schema.get_mut(*key) {
                let mut new_arr = Vec::new();
                for subschema in arr.iter() {
                    new_arr.push(self.expand_schema_references(subschema)?);
                }
                *arr = new_arr;
            }
        }

        if original_title.is_some() || original_description.is_some() {
            let obj = schema.as_object_mut().unwrap();
            if let Some(t) = original_title {
                obj.insert("title".to_string(), t);
            } else {
                obj.shift_remove("title");
            }
            if let Some(d) = original_description {
                obj.insert("description".to_string(), d);
            } else {
                obj.shift_remove("description");
            }
        }

        Ok(schema)
    }
}
