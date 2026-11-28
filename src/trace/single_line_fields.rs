//! Field formatting for the text log format that keeps every event on a single line.

use std::{
    error::Error,
    fmt::{self, Write as _},
};

use tracing_core::field::{Field, Visit};
use tracing_subscriber::{
    field::{MakeVisitor, VisitFmt, VisitOutput},
    fmt::format::{DefaultFields, DefaultVisitor, Writer},
};

/// Formats fields as [`DefaultFields`] does, except that carriage returns and line feeds inside
/// field values are written as `\r` and `\n`.
///
/// `DefaultFields` writes `Display` values and the message without escaping line breaks, so a value
/// that came from an external system could otherwise end the current log line and forge another.
#[derive(Debug, Default)]
pub(super) struct SingleLineFields(DefaultFields);

impl<'writer> MakeVisitor<Writer<'writer>> for SingleLineFields {
    type Visitor = SingleLineVisitor<'writer>;

    fn make_visitor(&self, target: Writer<'writer>) -> Self::Visitor {
        SingleLineVisitor(self.0.make_visitor(target))
    }
}

pub(super) struct SingleLineVisitor<'writer>(DefaultVisitor<'writer>);

impl Visit for SingleLineVisitor<'_> {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.record_f64(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.record_i64(field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.record_u64(field, value);
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.0.record_i128(field, value);
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.0.record_u128(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.record_bool(field, value);
    }

    fn record_bytes(&mut self, field: &Field, value: &[u8]) {
        self.0.record_bytes(field, value);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        // `DefaultVisitor` quotes string fields with `Debug`, which already escapes line breaks,
        // but writes a string message with `Display`.
        if field.name() == "message" {
            self.record_debug(field, &format_args!("{value}"));
        } else {
            self.0.record_str(field, value);
        }
    }

    fn record_error(&mut self, field: &Field, value: &(dyn Error + 'static)) {
        self.0.record_error(field, &EscapedError::new(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.record_debug(field, &EscapeLineBreaks(value));
    }
}

impl VisitOutput<fmt::Result> for SingleLineVisitor<'_> {
    fn finish(self) -> fmt::Result {
        self.0.finish()
    }
}

impl VisitFmt for SingleLineVisitor<'_> {
    fn writer(&mut self) -> &mut dyn fmt::Write {
        self.0.writer()
    }
}

/// An owned copy of an error chain with line breaks escaped.
///
/// A borrowing wrapper cannot be passed on as `dyn Error + 'static`, and passing the error on lets
/// `DefaultVisitor` keep its own escaping and layout for the error and its sources.
#[derive(Debug)]
struct EscapedError {
    message: Result<String, fmt::Error>,
    source: Option<Box<EscapedError>>,
}

impl EscapedError {
    fn new(error: &(dyn Error + 'static)) -> Self {
        let mut message = String::new();
        let message = write!(LineBreakEscaper(&mut message), "{error}").map(|()| message);
        let source = error.source().map(|source| Box::new(Self::new(source)));
        Self { message, source }
    }
}

impl fmt::Display for EscapedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message.as_deref().map_err(|&error| error)?)
    }
}

impl Error for EscapedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as _)
    }
}

struct EscapeLineBreaks<'a>(&'a dyn fmt::Debug);

impl fmt::Debug for EscapeLineBreaks<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(LineBreakEscaper(f), "{:?}", self.0)
    }
}

struct LineBreakEscaper<W>(W);

impl<W: fmt::Write> fmt::Write for LineBreakEscaper<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        s.split_inclusive(['\r', '\n']).try_for_each(|piece| {
            let (text, escaped) = if let Some(text) = piece.strip_suffix('\r') {
                (text, "\\r")
            } else if let Some(text) = piece.strip_suffix('\n') {
                (text, "\\n")
            } else {
                (piece, "")
            };
            self.0.write_str(text)?;
            self.0.write_str(escaped)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    use tracing::{error, info_span};
    use tracing_subscriber::{fmt::FormatFields, layer::SubscriberExt};

    use super::*;

    const INJECTED: &str = "boom\r\nERROR forged event";
    const ESCAPED: &str = r"boom\r\nERROR forged event";

    #[derive(Clone, Default)]
    struct CapturedOutput(Arc<Mutex<Vec<u8>>>);

    impl io::Write for CapturedOutput {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn render<F>(fields: F, ansi: bool, log: impl FnOnce()) -> String
    where
        F: for<'writer> FormatFields<'writer> + Send + Sync + 'static,
    {
        let output = CapturedOutput::default();
        let writer = output.clone();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .fmt_fields(fields)
                .with_ansi(ansi)
                .without_time()
                .with_writer(move || writer.clone()),
        );
        tracing::subscriber::with_default(subscriber, log);
        let bytes = output.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    /// Returns the single line that `log` produces, without its terminating newline.
    fn render_single_line(log: impl FnOnce()) -> String {
        let output = render(SingleLineFields::default(), false, log);
        let line = output
            .strip_suffix('\n')
            .unwrap_or_else(|| panic!("event should end with a newline: {output:?}"));
        assert!(
            !line.contains(['\r', '\n']),
            "event should not contain line breaks: {output:?}"
        );
        line.to_owned()
    }

    fn assert_escaped(log: impl FnOnce()) {
        let line = render_single_line(log);
        assert!(line.contains(ESCAPED), "{line:?}");
    }

    #[derive(Debug)]
    struct TestError {
        message: &'static str,
        source: Option<Box<TestError>>,
    }

    impl TestError {
        fn new(message: &'static str, source: &'static str) -> Self {
            Self {
                message,
                source: Some(Box::new(Self {
                    message: source,
                    source: None,
                })),
            }
        }
    }

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.message)
        }
    }

    impl Error for TestError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_deref().map(|source| source as _)
        }
    }

    #[test]
    fn escapes_display_field() {
        assert_escaped(|| error!(error = %INJECTED, "Request failed."));
    }

    #[test]
    fn escapes_debug_field() {
        assert_escaped(|| error!(error = ?INJECTED, "Request failed."));
    }

    #[test]
    fn escapes_string_field() {
        assert_escaped(|| error!(error = INJECTED, "Request failed."));
    }

    #[test]
    fn escapes_interpolated_message() {
        assert_escaped(|| error!("Request failed: {INJECTED}"));
    }

    #[test]
    fn escapes_display_message() {
        assert_escaped(|| error!(message = %INJECTED));
    }

    #[test]
    fn escapes_string_message() {
        assert_escaped(|| error!(message = INJECTED));
    }

    #[test]
    fn escapes_span_field() {
        assert_escaped(|| {
            let _entered = info_span!("request", peer = %INJECTED).entered();
            error!("Request failed.");
        });
    }

    #[test]
    fn escapes_error_field_and_sources() {
        let error = TestError::new(INJECTED, "root\ncause");
        let line = render_single_line(|| error!(error = &error as &dyn Error, "Request failed."));
        assert!(line.contains(ESCAPED), "{line:?}");
        assert!(line.contains(r"root\ncause"), "{line:?}");
    }

    #[test]
    fn keeps_ansi_escaping_of_error_field_and_sources() {
        let error = TestError::new("\x1b[31mred", "\x1b[32mgreen");
        let line = render_single_line(|| error!(error = &error as &dyn Error, "Request failed."));
        assert!(!line.contains('\x1b'), "{line:?}");
        assert!(line.contains(r"\x1b[31mred"), "{line:?}");
        assert!(line.contains(r"\x1b[32mgreen"), "{line:?}");
    }

    fn assert_matches_default_fields_without_line_breaks(ansi: bool) {
        let log = || {
            let _entered = info_span!("request", peer = %"127.0.0.1", attempt = 2).entered();
            error!(
                error = %"refused",
                status = ?Some(503),
                path = "/v1/input",
                retry = true,
                elapsed = 1.5,
                "Request failed: {}.",
                "connection refused",
            );
            error!(message = "Plain \x1b[31mstring\x1b[0m message.");
            let error = TestError::new("request failed", "connection \x1b[31mrefused");
            error!(error = &error as &dyn Error, "Request failed.");
        };
        assert_eq!(
            render(SingleLineFields::default(), ansi, log),
            render(DefaultFields::new(), ansi, log)
        );
    }

    #[test]
    fn matches_default_fields_without_line_breaks() {
        assert_matches_default_fields_without_line_breaks(false);
    }

    #[test]
    fn matches_default_fields_without_line_breaks_with_ansi() {
        assert_matches_default_fields_without_line_breaks(true);
    }
}
