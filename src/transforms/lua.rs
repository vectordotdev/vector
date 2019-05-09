use super::Transform;
use crate::record::Record;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct LuaConfig {
    source: String,
}

#[typetag::serde(name = "lua")]
impl crate::topology::config::TransformConfig for LuaConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(Lua::new(self.source.clone())))
    }
}

pub struct Lua {
    lua: rlua::Lua,
}

impl Lua {
    pub fn new(source: String) -> Self {
        let lua = rlua::Lua::new();

        lua.context(|ctx| {
            let func = ctx.load(&source).into_function().unwrap();
            ctx.set_named_registry_value("vector_func", func).unwrap();
        });

        Self { lua }
    }

    fn process(&self, record: Record) -> Result<Option<Record>, rlua::Error> {
        self.lua.context(|ctx| {
            let globals = ctx.globals();

            globals.set("record", record)?;

            let func = ctx.named_registry_value::<_, rlua::Function>("vector_func")?;
            func.call(())?;

            globals.get::<_, Option<Record>>("record")
        })
    }
}

impl Transform for Lua {
    fn transform(&self, record: Record) -> Option<Record> {
        match self.process(record) {
            Ok(record) => record,
            Err(err) => {
                error!(
                    "Error in lua script; discarding record.\n{}",
                    format_error(&err)
                );
                None
            }
        }
    }
}

impl rlua::UserData for Record {
    fn add_methods<'lua, M: rlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_meta_method_mut(
            rlua::MetaMethod::NewIndex,
            |_ctx, this, (key, value): (String, Option<rlua::String<'lua>>)| {
                if let Some(string) = value {
                    this.insert_explicit(key.into(), string.as_bytes().into());
                } else {
                    this.remove(&key.into());
                }

                Ok(())
            },
        );

        methods.add_meta_method(rlua::MetaMethod::Index, |ctx, this, key: String| {
            if let Some(value) = this.get(&key.into()) {
                let string = ctx.create_string(&value.as_bytes())?;
                Ok(Some(string))
            } else {
                Ok(None)
            }
        });
    }
}

fn format_error(error: &rlua::Error) -> String {
    match error {
        rlua::Error::CallbackError { traceback, cause } => format_error(&cause) + "\n" + traceback,
        err => err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_error, Lua};
    use crate::{record::Record, transforms::Transform};

    #[test]
    fn lua_add_field() {
        let transform = Lua::new(
            r#"
          record["hello"] = "goodbye"
        "#
            .to_owned(),
        );

        let record = Record::from("program me");

        let record = transform.transform(record).unwrap();

        assert_eq!(record[&"hello".into()], "goodbye".into());
    }

    #[test]
    fn lua_read_field() {
        let transform = Lua::new(
            r#"
          _, _, name = string.find(record["message"], "Hello, my name is (%a+).")
          record["name"] = name
        "#
            .to_owned(),
        );

        let record = Record::from("Hello, my name is Bob.");

        let record = transform.transform(record).unwrap();

        assert_eq!(record[&"name".into()], "Bob".into());
    }

    #[test]
    fn lua_remove_field() {
        let transform = Lua::new(
            r#"
          record["name"] = nil
        "#
            .to_owned(),
        );

        let mut record = Record::new_empty();
        record.insert_explicit("name".into(), "Bob".into());
        let record = transform.transform(record).unwrap();

        assert!(record.get(&"name".into()).is_none());
    }

    #[test]
    fn lua_drop_record() {
        let transform = Lua::new(
            r#"
          record = nil
        "#
            .to_owned(),
        );

        let mut record = Record::new_empty();
        record.insert_explicit("name".into(), "Bob".into());
        let record = transform.transform(record);

        assert!(record.is_none());
    }

    #[test]
    fn lua_read_empty_field() {
        let transform = Lua::new(
            r#"
          if record["non-existant"] == nil then
            record["result"] = "empty"
          else
            record["result"] = "found"
          end
        "#
            .to_owned(),
        );

        let record = Record::new_empty();
        let record = transform.transform(record).unwrap();

        assert_eq!(record[&"result".into()], "empty".into());
    }

    #[test]
    fn lua_numeric_value() {
        let transform = Lua::new(
            r#"
          record["number"] = 3
        "#
            .to_owned(),
        );

        let record = transform.transform(Record::new_empty()).unwrap();
        assert_eq!(record[&"number".into()], "3".into());
    }

    #[test]
    fn lua_non_coercible_value() {
        let transform = Lua::new(
            r#"
          record["junk"] = {"asdf"}
        "#
            .to_owned(),
        );

        let err = transform.process(Record::new_empty()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("error converting Lua table to String"), err);
    }

    #[test]
    fn lua_non_string_key_write() {
        let transform = Lua::new(
            r#"
          record[false] = "hello"
        "#
            .to_owned(),
        );

        let err = transform.process(Record::new_empty()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("error converting Lua boolean to String"), err);
    }

    #[test]
    fn lua_non_string_key_read() {
        let transform = Lua::new(
            r#"
          print(record[false])
        "#
            .to_owned(),
        );

        let err = transform.process(Record::new_empty()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("error converting Lua boolean to String"), err);
    }

    #[test]
    fn lua_script_error() {
        let transform = Lua::new(
            r#"
          error("this is an error")
        "#
            .to_owned(),
        );

        let err = transform.process(Record::new_empty()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("this is an error"), err);
    }
}
