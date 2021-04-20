use crate::sources::{
    socket::{Mode,SocketConfig},
    util::StreamDecoder,
};
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    Pipeline,
    transforms::remap::{Remap, RemapConfig},
};
use bytes::Bytes;
use indoc::indoc;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct SyslogConfig {
    // Config settings we may need access to
    // host_key: Option<String>,
    #[serde(flatten)]
    original_config: SocketConfig,
}

inventory::submit! {
    SourceDescription::new::<SyslogConfig>("syslog")
}

impl GenerateConfig for SyslogConfig {
    fn generate_config() -> toml::Value {
        SocketConfig::generate_config()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "syslog")]
impl SourceConfig for SyslogConfig {
    async fn build(&self, mut cx: SourceContext) -> crate::Result<super::Source> {
        let conf = RemapConfig {
            source: indoc! {r#"
                structured = parse_syslog!(.message)
                . = merge(., structured)
            "#}
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
                    .insert(log_schema().source_type_key(), Bytes::from("syslog"));
                Ok(event)
            })
            .forward(out)
            .await
        });

        match self.original_config.mode.clone() {
            Mode::Tcp(mut config) => {
                config.set_decoder(
                    Some(StreamDecoder::SyslogDecoder(codec::SyslogDecoder::new(config.max_length())))
                );
                SocketConfig::new_tcp(config).build(cx).await
            },
            //Mode::UnixStream() => {
                // do something
            //},
            _ => self.original_config.build(cx).await,
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "syslog"
    }

    fn resources(&self) -> Vec<Resource> {
        self.original_config.resources()
    }
}

#[cfg(test)]
mod test {
    use super::SyslogConfig;
    use crate::sources::socket::Mode;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SyslogConfig>();
    }

    #[test]
    fn config_tcp() {
        let config: SyslogConfig = toml::from_str(
            r#"
            mode = "tcp"
            address = "127.0.0.1:1235"
          "#,
        )
        .unwrap();
        assert!(matches!(config.original_config.mode, Mode::Tcp(_)));
    }
}
