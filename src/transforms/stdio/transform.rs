use std::pin::Pin;

use futures::Stream;
use vector_lib::{
    codecs::encoding,
    config::{DataType, Input, OutputId, TransformOutput},
    event::Event,
    schema,
    transform::{TaskTransform, Transform},
};

use crate::{
    codecs::{DecodingConfig, Encoder, SinkType},
    config::{TransformConfig, TransformContext},
    transforms::stdio::{
        config::{Mode, StderrMode, StdioConfig},
        exec::{ExecTask, OsExecTask},
        process::{OsSpawner, Spawner},
    },
};

#[async_trait::async_trait]
#[typetag::serde(name = "stdio")]
impl TransformConfig for StdioConfig {
    async fn build(&self, cx: &TransformContext) -> crate::Result<Transform> {
        let (framer, serializer) = self.stdin.encoding.build(SinkType::StreamBased)?;
        let stdin_encoder = Encoder::<encoding::Framer>::new(framer, serializer);

        let stdout_decoder = DecodingConfig::new(
            self.stdout
                .framing
                .clone()
                .unwrap_or(self.stdout.decoding.default_stream_framing()),
            self.stdout.decoding.clone(),
            cx.log_namespace(self.stdout.log_namespace),
        )
        .build()?;

        let stderr_decoder = matches!(self.stderr.mode, StderrMode::Forward)
            .then_some(DecodingConfig::new(
                self.stderr
                    .framing
                    .clone()
                    .unwrap_or(self.stderr.decoding.default_stream_framing()),
                self.stderr.decoding.clone(),
                cx.log_namespace(self.stderr.log_namespace),
            ))
            .map(|config| config.build())
            .transpose()?;

        Ok(Transform::event_task(OsExecTask {
            command: self.command.clone(),
            mode: self.mode,
            scheduled: self.scheduled,
            streaming: self.streaming,
            per_event: self.per_event,
            capture_stderr: matches!(self.stderr.mode, StderrMode::Forward),
            spawner: OsSpawner,
            stdin_encoder,
            stdout_decoder,
            stderr_decoder,
        }))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(
        &self,
        _: &TransformContext,
        input_definitions: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::all_bits(),
            input_definitions
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )]
    }

    fn enable_concurrency(&self) -> bool {
        true
    }

    fn validate(&self, _: &schema::Definition) -> Result<(), Vec<String>> {
        let mut errors = vec![];

        if self
            .per_event
            .is_some_and(|c| c.max_concurrent_processes == 0)
        {
            errors.push("per_event.max_concurrent_processes must be greater than 0");
        }

        if self.command.command.is_empty() {
            errors.push("command must contain at least one element");
        }

        if errors.is_empty() {
            return Ok(());
        }

        Err(errors.into_iter().map(str::to_string).collect())
    }
}

impl<S: Spawner> TaskTransform<Event> for ExecTask<S> {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        match self.mode {
            Mode::PerEvent => Box::pin(self.run_per_event(task)),
            Mode::Scheduled => Box::pin(self.run_scheduled(task)),
            Mode::Streaming => Box::pin(self.run_streaming(task)),
        }
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::schema::Definition;

    use crate::transforms::stdio::config::PerEventConfig;

    use super::*;

    #[test]
    fn test_config_validation() {
        let mut config = StdioConfig::default();

        config.command.command = vec!["echo".into()];
        assert!(config.validate(&Definition::any()).is_ok());

        config.per_event = Some(PerEventConfig {
            max_concurrent_processes: 0,
        });
        let result = config.validate(&Definition::any());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err()[0],
            "per_event.max_concurrent_processes must be greater than 0"
        );
        config.per_event = None;

        config.command.command = vec![];
        let result = config.validate(&Definition::any());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err()[0],
            "command must contain at least one element"
        );
    }
}
