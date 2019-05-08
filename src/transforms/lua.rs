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
}

impl Transform for Lua {
    fn transform(&self, record: Record) -> Option<Record> {
        let record = self
            .lua
            .context(|ctx| {
                let globals = ctx.globals();

                globals.set("record", record)?;

                let func = ctx.named_registry_value::<_, rlua::Function>("vector_func")?;
                func.call(())?;

                globals.get::<_, Record>("record")
            })
            .unwrap();

        Some(record)
    }
}

impl rlua::UserData for Record {
    fn add_methods<'lua, M: rlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_meta_method_mut(
            rlua::MetaMethod::NewIndex,
            |_ctx, this, (key, value): (String, rlua::String<'lua>)| {
                this.insert_explicit(key.into(), value.as_bytes().into());
                Ok(())
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Lua;
    use crate::{record::Record, transforms::Transform};

    #[test]
    fn lua_add_field() {
        let transform = Lua::new("record['hello'] = 'goodbye'".to_owned());

        let record = Record::from("program me");

        let record = transform.transform(record).unwrap();

        assert_eq!(record[&"hello".into()], "goodbye".into());
    }
}
