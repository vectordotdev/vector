use mlua::prelude::*;
use std::collections::BTreeMap;
use crate::event::{EventMetadata,TraceEvent};

impl<'a> ToLua<'a> for TraceEvent {
    #![allow(clippy::wrong_self_convention)] // this trait is defined by mlua
    fn to_lua(self, lua: &'a Lua) -> LuaResult<LuaValue> {
        let (value, _metadata) = self.into_parts();
        value.to_lua(lua)
    }
}

impl<'a> FromLua<'a> for TraceEvent {
    fn from_lua(lua_value: LuaValue<'a>, lua: &'a Lua) -> LuaResult<Self> {
        //let value = Value::from_lua(lua_value, lua)?;
        Ok(TraceEvent::from_parts(BTreeMap::from_lua(lua_value, lua)?, EventMetadata::default()))
    }
}
