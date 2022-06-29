mod cri;
mod docker;
mod test_util;

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
}

impl Parser {
    pub const fn new() -> Self {
        Self {
            state: ParserState::Uninitialized,
        }
    }
}

impl FunctionTransform for Parser {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        match &mut self.state {
            ParserState::Uninitialized => {
                let message = match event
                    .as_log()
                    .get(crate::config::log_schema().message_key())
                {
                    Some(message) => message,
                    None => {
                        emit!(KubernetesLogsFormatPickerEdgeCase {
                            what: "got an event with no message field"
                        });
                        return;
                    }
                };

                let bytes = match message {
                    Value::Bytes(bytes) => bytes,
                    _ => {
                        emit!(KubernetesLogsFormatPickerEdgeCase {
                            what: "got an event with non-bytes message field"
                        });
                        return;
                    }
                };

                self.state = if bytes.len() > 1 && bytes[0] == b'{' {
                    ParserState::Docker(docker::Docker)
                } else {
                    ParserState::Cri(cri::Cri::default())
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
    use super::*;
    use crate::{event::Event, event::LogEvent, test_util::trace_init, transforms::Transform};

    /// Picker has to work for all test cases for underlying parsers.
    fn cases() -> Vec<(String, Vec<Event>)> {
        let mut cases = vec![];
        cases.extend(docker::tests::cases());
        cases.extend(cri::tests::cases());
        cases
    }

    #[test]
    fn test_parsing() {
        trace_init();
        test_util::test_parser(|| Transform::function(Parser::new()), Event::from, cases());
    }

    #[test]
    fn test_parsing_invalid() {
        trace_init();

        let cases = vec!["", "qwe", "{"];

        for message in cases {
            let input = Event::from(message);
            let mut parser = Parser::new();
            let mut output = OutputBuffer::default();
            parser.transform(&mut output, input);
            assert!(output.is_empty(), "Expected no events: {:?}", output);
        }
    }

    #[test]
    fn test_parsing_invalid_non_standard_events() {
        trace_init();

        let cases = vec![
            // No `message` field.
            Event::from(LogEvent::default()),
            // Non-bytes `message` field.
            {
                let mut input = LogEvent::default();
                input.insert("message", 123);
                input.into()
            },
        ];

        for input in cases {
            let mut parser = Parser::new();
            let mut output = OutputBuffer::default();
            parser.transform(&mut output, input);
            assert!(output.is_empty(), "Expected no events: {:?}", output);
        }
    }
}
