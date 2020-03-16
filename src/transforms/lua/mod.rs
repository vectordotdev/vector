pub mod v1;
pub mod v2;

use crate::{
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    transforms::Transform,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
enum V1 {
    #[serde(rename = "1")]
    V1,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct LuaConfigV1 {
    version: Option<V1>,
    #[serde(flatten)]
    config: v1::LuaConfig,
}

#[derive(Serialize, Deserialize, Debug)]
enum V2 {
    #[serde(rename = "2")]
    V2,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct LuaConfigV2 {
    version: V2,
    #[serde(flatten)]
    config: v2::LuaConfig,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum LuaConfig {
    V1(LuaConfigV1),
    V2(LuaConfigV2),
}

inventory::submit! {
    TransformDescription::new_without_default::<LuaConfig>("lua")
}

#[typetag::serde(name = "lua")]
impl TransformConfig for LuaConfig {
    fn build(&self, cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        match self {
            LuaConfig::V1(v1) => v1.config.build(cx),
            LuaConfig::V2(v2) => v2.config.build(cx),
        }
    }

    fn input_type(&self) -> DataType {
        match self {
            LuaConfig::V1(v1) => v1.config.input_type(),
            LuaConfig::V2(v2) => v2.config.input_type(),
        }
    }

    fn output_type(&self) -> DataType {
        match self {
            LuaConfig::V1(v1) => v1.config.output_type(),
            LuaConfig::V2(v2) => v2.config.output_type(),
        }
    }

    fn transform_type(&self) -> &'static str {
        match self {
            LuaConfig::V1(v1) => v1.config.transform_type(),
            LuaConfig::V2(v2) => v2.config.transform_type(),
        }
    }
}
