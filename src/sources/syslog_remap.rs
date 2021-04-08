use crate::sources::socket::SocketConfig;
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    shutdown::ShutdownSignal,
    transforms::remap::{Remap, RemapConfig},
    Pipeline,
};
use bytes::Bytes;

use futures::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct SyslogRemapConfig {
    // Config settings we may need access to
    // host_key: Option<String>,
    #[serde(flatten)]
    original_config: SocketConfig,
}

inventory::submit! {
    SourceDescription::new::<SyslogRemapConfig>("syslog_remap")
}

impl GenerateConfig for SyslogRemapConfig {
    fn generate_config() -> toml::Value {
        return SocketConfig::generate_config();
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "syslog_remap")]
impl SourceConfig for SyslogRemapConfig {
    async fn build(&self, mut cx: SourceContext) -> crate::Result<super::Source> {
        let conf = RemapConfig {
            source: r#"
structured = parse_syslog!(.message)
. = merge(., structured)
"#
            .to_string(),
            drop_on_abort: false,
            drop_on_error: false,
        };
        let tf = Remap::new(conf).unwrap();
        let (to_transform, rx) = Pipeline::new_with_buffer(100, vec![Box::new(tf)]);
        let out = cx.out;
        cx.out = to_transform;
        tokio::spawn(async move {
            rx.map(|mut event| {
                event
                    .as_mut_log()
                    .insert(log_schema().source_type_key(), Bytes::from("syslog_remap"));
                Ok(event)
            })
            .forward(out)
            .await
        });
        self.original_config.build(cx).await
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

#[cfg(test)]
mod test {
    use super::SyslogRemapConfig;
    use crate::sources::socket::Mode;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SyslogRemapConfig>();
    }

    #[test]
    fn config_tcp() {
        let config: SyslogRemapConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(matches!(config.original_config.mode, Mode::Tcp(_)));
    }
}
