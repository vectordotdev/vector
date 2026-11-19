use vector_lib::internal_event::emit;
mod cri;
mod docker;
mod test_util;

use vector_lib::config::LogNamespace;

use crate::{
    event::{Event, Value},
    internal_events::KubernetesLogsFormatPickerEdgeCase,
    sources::kubernetes_logs::transform_utils::get_message_path,
    transforms::{FunctionTransform, OutputBuffer},
};

/// Runtime format detected by one parser execution.
#[derive(Clone, Debug)]
pub(super) enum ParserState {
    /// Runtime has not yet been detected.
    Uninitialized,

    /// Docker runtime is being used.
    Docker,

    /// CRI is being used.
    Cri,
}

#[derive(Clone, Debug)]
pub(super) struct Parser {
    log_namespace: LogNamespace,
}

impl Parser {
    pub(super) const fn new(log_namespace: LogNamespace) -> Self {
        Self { log_namespace }
    }
}

impl FunctionTransform for Parser {
    type State = ParserState;

    fn new_state(&self) -> ParserState {
        ParserState::Uninitialized
    }

    fn transform(&self, state: &mut ParserState, output: &mut OutputBuffer, event: Event) {
        match state {
            ParserState::Uninitialized => {
                let message_field = get_message_path(self.log_namespace);
                let Some(message) = event.as_log().get(&message_field) else {
                    emit(KubernetesLogsFormatPickerEdgeCase {
                        what: "got an event without a message",
                    });
                    return;
                };

                let bytes = match message {
                    Value::Bytes(bytes) => bytes,
                    Value::String(string) => string.as_bytes(),
                    _ => {
                        emit(KubernetesLogsFormatPickerEdgeCase {
                            what: "got an event with non-bytes message",
                        });
                        return;
                    }
                };

                *state = if bytes.len() > 1 && bytes[0] == b'{' {
                    ParserState::Docker
                } else {
                    ParserState::Cri
                };
                self.transform(state, output, event);
            }
            ParserState::Docker => {
                docker::Docker::new(self.log_namespace).transform(&mut (), output, event);
            }
            ParserState::Cri => {
                cri::Cri::new(self.log_namespace).transform(&mut (), output, event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use vector_lib::lookup::event_path;
    use vrl::value;

    use super::*;
    use crate::{
        event::{Event, LogEvent},
        test_util::trace_init,
    };

    /// Picker has to work for all test cases for underlying parsers.
    fn valid_cases(log_namespace: LogNamespace) -> Vec<(Bytes, Vec<Event>)> {
        let mut valid_cases = vec![];
        valid_cases.extend(docker::tests::valid_cases(log_namespace));
        valid_cases.extend(cri::tests::valid_cases(log_namespace));
        valid_cases
    }

    fn invalid_cases() -> Vec<Bytes> {
        let mut invalid_cases = vec![];
        invalid_cases.extend(docker::tests::invalid_cases());
        invalid_cases
    }

    #[test]
    fn shared_parser_keeps_format_detection_in_each_state() {
        trace_init();
        let parser = Parser::new(LogNamespace::Legacy);
        let mut docker_state = parser.new_state();
        let mut cri_state = parser.new_state();
        let (docker_input, docker_expected) = docker::tests::valid_cases(LogNamespace::Legacy)
            .into_iter()
            .next()
            .unwrap();
        let (cri_input, cri_expected) = cri::tests::valid_cases(LogNamespace::Legacy)
            .into_iter()
            .next()
            .unwrap();
        let mut output = OutputBuffer::default();
        parser.transform(
            &mut docker_state,
            &mut output,
            LogEvent::from(docker_input).into(),
        );
        assert!(matches!(docker_state, ParserState::Docker));
        assert_eq!(output.into_events().collect::<Vec<_>>(), docker_expected);

        let mut output = OutputBuffer::default();
        parser.transform(
            &mut cri_state,
            &mut output,
            LogEvent::from(cri_input.clone()).into(),
        );
        assert!(matches!(cri_state, ParserState::Cri));
        assert_eq!(output.into_events().collect::<Vec<_>>(), cri_expected);

        // Once selected, the format is retained for subsequent events.
        let mut output = OutputBuffer::default();
        parser.transform(
            &mut docker_state,
            &mut output,
            LogEvent::from(cri_input).into(),
        );
        assert!(matches!(docker_state, ParserState::Docker));
        assert!(output.is_empty());
    }

    #[test]
    fn test_parsing_valid_vector_namespace() {
        trace_init();
        test_util::test_parser(
            || Parser::new(LogNamespace::Vector),
            |bytes| Event::Log(LogEvent::from(value!(bytes))),
            valid_cases(LogNamespace::Vector),
        );
    }

    #[test]
    fn test_parsing_valid_legacy_namespace() {
        trace_init();
        test_util::test_parser(
            || Parser::new(LogNamespace::Legacy),
            |bytes| Event::Log(LogEvent::from(bytes)),
            valid_cases(LogNamespace::Legacy),
        );
    }

    #[test]
    fn test_parsing_invalid_legacy_namespace() {
        trace_init();

        let cases = invalid_cases();

        for bytes in cases {
            let parser = Parser::new(LogNamespace::Legacy);
            let mut parser_state = parser.new_state();
            let input = LogEvent::from(bytes);
            let mut output = OutputBuffer::default();
            parser.transform(&mut parser_state, &mut output, input.into());

            assert!(output.is_empty(), "Expected no events: {output:?}");
        }
    }

    #[test]
    fn test_parsing_invalid_non_standard_events() {
        trace_init();

        let cases = vec![
            // No `message` field.
            (LogEvent::default(), LogNamespace::Legacy),
            // Non-bytes `message` field.
            (LogEvent::from(value!(123)), LogNamespace::Vector),
            (
                {
                    let mut input = LogEvent::default();
                    input.insert(event_path!("message"), 123);
                    input
                },
                LogNamespace::Legacy,
            ),
        ];

        for (input, log_namespace) in cases {
            let parser = Parser::new(log_namespace);
            let mut parser_state = parser.new_state();
            let mut output = OutputBuffer::default();
            parser.transform(&mut parser_state, &mut output, input.into());

            assert!(output.is_empty(), "Expected no events: {output:?}");
        }
    }
}
