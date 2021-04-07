use crate::sources::syslog::SyslogConfig;
use crate::{
    config::{
        DataType, GenerateConfig, GlobalOptions, Resource, SourceConfig,
        SourceDescription,
    },
    transforms::remap::{
        Remap,RemapConfig,
    },
    shutdown::ShutdownSignal,
    Pipeline,
};

use serde::{Deserialize, Serialize};


#[derive(Deserialize, Serialize, Debug)]
pub struct SyslogRemapConfig {
    #[serde(flatten)]
    original_config: SyslogConfig,
}

inventory::submit! {
    SourceDescription::new::<SyslogRemapConfig>("syslog_remap")
}

impl GenerateConfig for SyslogRemapConfig {
    fn generate_config() -> toml::Value {
        return SyslogConfig::generate_config()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "syslog_remap")]
impl SourceConfig for SyslogRemapConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {

        let conf = RemapConfig {
            source: r#"
. = parse_syslog!(.message)
"#
            .to_string(),
            drop_on_abort: false,
            drop_on_error: false,
        };
        let mut tf = Remap::new(conf).unwrap();
        let (mut to_transform, rx) =
            Pipeline::new_with_buffer(100, vec![Box::new(tf)]);
        self.original_config.build(_name, _globals, shutdown.clone(), to_transform);

        Ok(Box::pin(async move {
            Ok(
                loop {
                    tokio::select! {
                        // Todo :
                        // Wire rx to out
                        _ = shutdown => break

                    }
                }
            )
        }))

    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "syslog_remap"
    }

    fn resources(&self) -> Vec<Resource> {
        self.original_config.resources()
    }
}
