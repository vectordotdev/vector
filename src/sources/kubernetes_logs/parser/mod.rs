mod cri;
mod docker;
mod test_util;

use vector_lib::config::LogNamespace;

use crate::sources::kubernetes_logs::transform_utils::get_message_path;
use crate::{
    event::{Event, Value},
    internal_events::KubernetesLogsFormatPickerEdgeCase,
    transforms::{FunctionTransform, OutputBuffer},
};

#[derive(Clone, Debug)]
enum ParserState {
    /// Runtime has not yet been detected.
    Uninitialized,

    /// Docker runtime is being used.
    Docker(docker::Docker),

    /// CRI is being used.
    Cri(cri::Cri),
}

#[derive(Clone, Debug)]
pub struct Parser {
    state: ParserState,
    log_namespace: LogNamespace,
}

impl Parser {
    pub const fn new(log_namespace: LogNamespace) -> Self {
        Self {
            state: ParserState::Uninitialized,
            log_namespace,
        }
    }
}

impl FunctionTransform for Parser {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        match &mut self.state {
            ParserState::Uninitialized => {
                let message_field = get_message_path(self.log_namespace);
                let message = match event.as_log().get(&message_field) {
                    Some(message) => message,
                    None => {
                        emit!(KubernetesLogsFormatPickerEdgeCase {
                            what: "got an event without a message"
                        });
                        return;
                    }
                };

                let bytes = match message {
                    Value::Bytes(bytes) => bytes,
                    _ => {
                        emit!(KubernetesLogsFormatPickerEdgeCase {
                            what: "got an event with non-bytes message"
                        });
                        return;
                    }
                };

                self.state = if bytes.len() > 1 && bytes[0] == b'{' {
                    ParserState::Docker(docker::Docker::new(self.log_namespace))
                } else {
                    ParserState::Cri(cri::Cri::new(self.log_namespace))
                };
                self.transform(output, event)
            }
            ParserState::Docker(t) => t.transform(output, event),
            ParserState::Cri(t) => t.transform(output, event),
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use vector_lib::lookup::event_path;
    use vrl::value;

    use super::*;
    use crate::{event::Event, event::LogEvent, test_util::trace_init};

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
            let mut parser = Parser::new(LogNamespace::Legacy);
            let input = LogEvent::from(bytes);
            let mut output = OutputBuffer::default();
            parser.transform(&mut output, input.into());

            assert!(output.is_empty(), "Expected no events: {:?}", output);
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
            let mut parser = Parser::new(log_namespace);
            let mut output = OutputBuffer::default();
            parser.transform(&mut output, input.into());

            assert!(output.is_empty(), "Expected no events: {:?}", output);
        }
    }
}
