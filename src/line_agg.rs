//! A reusable line aggregation implementation.

#![deny(missing_docs)]

use std::{
    collections::{hash_map::Entry, HashMap},
    hash::Hash,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use bytes::{Bytes, BytesMut};
use futures::{Stream, StreamExt};
use pin_project::pin_project;
use regex::bytes::Regex;
use tokio_util::time::delay_queue::{DelayQueue, Key};
use vector_lib::configurable::configurable_component;

/// Mode of operation of the line aggregator.
#[configurable_component]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// All consecutive lines matching this pattern are included in the group.
    ///
    /// The first line (the line that matched the start pattern) does not need to match the `ContinueThrough` pattern.
    ///
    /// This is useful in cases such as a Java stack trace, where some indicator in the line (such as a leading
    /// whitespace) indicates that it is an extension of the proceeding line.
    ContinueThrough,

    /// All consecutive lines matching this pattern, plus one additional line, are included in the group.
    ///
    /// This is useful in cases where a log message ends with a continuation marker, such as a backslash, indicating
    /// that the following line is part of the same message.
    ContinuePast,

    /// All consecutive lines not matching this pattern are included in the group.
    ///
    /// This is useful where a log line contains a marker indicating that it begins a new message.
    HaltBefore,

    /// All consecutive lines, up to and including the first line matching this pattern, are included in the group.
    ///
    /// This is useful where a log line ends with a termination marker, such as a semicolon.
    HaltWith,
}

/// Configuration of multi-line aggregation.
#[derive(Clone, Debug)]
pub struct Config {
    /// Regular expression pattern that is used to match the start of a new message.
    pub start_pattern: Regex,

    /// Regular expression pattern that is used to determine whether or not more lines should be read.
    ///
    /// This setting must be configured in conjunction with `mode`.
    pub condition_pattern: Regex,

    /// Aggregation mode.
    ///
    /// This setting must be configured in conjunction with `condition_pattern`.
    pub mode: Mode,

    /// The maximum amount of time to wait for the next additional line, in milliseconds.
    ///
    /// Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete.
    pub timeout: Duration,
}

impl Config {
    /// Build `Config` from legacy `file` source line aggregator configuration
    /// params.
    pub fn for_legacy(marker: Regex, timeout_ms: u64) -> Self {
        let start_pattern = marker;
        let condition_pattern = start_pattern.clone();
        let mode = Mode::HaltBefore;
        let timeout = Duration::from_millis(timeout_ms);

        Self {
            start_pattern,
            condition_pattern,
            mode,
            timeout,
        }
    }
}

/// Line aggregator.
///
/// Provides a `Stream` implementation that reads lines from the `inner` stream
/// and yields aggregated lines.
#[pin_project(project = LineAggProj)]
pub struct LineAgg<T, K, C> {
    /// The stream from which we read the lines.
    #[pin]
    inner: T,

    /// The core line aggregation logic.
    logic: Logic<K, C>,

    /// Stashed lines. When line aggregation results in more than one line being
    /// emitted, we have to stash lines and return them into the stream after
    /// that before doing any other work.
    stashed: Option<(K, Bytes, C)>,

    /// Draining queue. We switch to draining mode when we get `None` from
    /// the inner stream. In this mode we stop polling `inner` for new lines
    /// and just flush all the buffered data.
    draining: Option<Vec<(K, Bytes, C, Option<C>)>>,
}

/// Core line aggregation logic.
///
/// Encapsulates the essential state and the core logic for the line
/// aggregation algorithm.
pub struct Logic<K, C> {
    /// Configuration parameters to use.
    config: Config,

    /// Line per key.
    /// Key is usually a filename or other line source identifier.
    buffers: HashMap<K, (Key, Aggregate<C>)>,

    /// A queue of key timeouts.
    timeouts: DelayQueue<K>,
}

impl<K, C> Logic<K, C> {
    /// Create a new `Logic` using the specified `Config`.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            buffers: HashMap::new(),
            timeouts: DelayQueue::new(),
        }
    }
}

impl<T, K, C> LineAgg<T, K, C>
where
    T: Stream<Item = (K, Bytes, C)> + Unpin,
    K: Hash + Eq + Clone,
{
    /// Create a new `LineAgg` using the specified `inner` stream and
    /// preconfigured `logic`.
    pub const fn new(inner: T, logic: Logic<K, C>) -> Self {
        Self {
            inner,
            logic,
            draining: None,
            stashed: None,
        }
    }
}

impl<T, K, C> Stream for LineAgg<T, K, C>
where
    T: Stream<Item = (K, Bytes, C)> + Unpin,
    K: Hash + Eq + Clone,
{
    /// `K` - file name, or other line source,
    /// `Bytes` - the line data,
    /// `C` - the initial context related to the first line of data.
    /// `Option<C>` - context related to the last-seen line data.
    type Item = (K, Bytes, C, Option<C>);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            // If we have a stashed line, process it before doing anything else.
            if let Some((src, line, context)) = this.stashed.take() {
                // Handle the stashed line. If the handler gave us something -
                // return it, otherwise restart the loop iteration to start
                // anew. Handler could've stashed another value, continuing to
                // the new loop iteration handles that.
                if let Some(val) = Self::handle_line_and_stashing(&mut this, src, line, context) {
                    return Poll::Ready(Some(val));
                }
                continue;
            }

            // If we're in draining mode, short circuit here.
            if let Some(to_drain) = &mut this.draining {
                if let Some(val) = to_drain.pop() {
                    return Poll::Ready(Some(val));
                } else {
                    return Poll::Ready(None);
                }
            }

            match this.inner.poll_next_unpin(cx) {
                Poll::Ready(Some((src, line, context))) => {
                    // Handle the incoming line we got from `inner`. If the
                    // handler gave us something - return it, otherwise continue
                    // with the flow.
                    if let Some(val) = Self::handle_line_and_stashing(&mut this, src, line, context)
                    {
                        return Poll::Ready(Some(val));
                    }
                }
                Poll::Ready(None) => {
                    // We got `None`, this means the `inner` stream has ended.
                    // Start flushing all existing data, stop polling `inner`.
                    *this.draining = Some(
                        this.logic
                            .buffers
                            .drain()
                            .map(|(src, (_, aggregate))| {
                                let (line, initial_context, last_context) = aggregate.merge();
                                (src, line, initial_context, last_context)
                            })
                            .collect(),
                    );
                }
                Poll::Pending => {
                    // We didn't get any lines from `inner`, so we just give
                    // a line from keys that have hit their timeout.
                    while let Poll::Ready(Some(expired_key)) = this.logic.timeouts.poll_expired(cx)
                    {
                        let key = expired_key.into_inner();
                        if let Some((_, aggregate)) = this.logic.buffers.remove(&key) {
                            let (line, initial_context, last_context) = aggregate.merge();
                            return Poll::Ready(Some((key, line, initial_context, last_context)));
                        }
                    }

                    return Poll::Pending;
                }
            };
        }
    }
}

impl<T, K, C> LineAgg<T, K, C>
where
    T: Stream<Item = (K, Bytes, C)> + Unpin,
    K: Hash + Eq + Clone,
{
    /// Handle line and do stashing of extra emitted lines.
    /// Requires that the `stashed` item is empty (i.e. entry is vacant). This
    /// invariant has to be taken care of by the caller.
    fn handle_line_and_stashing(
        this: &mut LineAggProj<'_, T, K, C>,
        src: K,
        line: Bytes,
        context: C,
    ) -> Option<(K, Bytes, C, Option<C>)> {
        // Stashed line is always consumed at the start of the `poll`
        // loop before entering this line processing logic. If it's
        // non-empty here - it's a bug.
        debug_assert!(this.stashed.is_none());
        let val = this.logic.handle_line(src, line, context)?;
        let val = match val {
            // If we have to emit just one line - that's easy,
            // we just return it.
            (src, Emit::One((line, initial_context, last_context))) => {
                (src, line, initial_context, last_context)
            }
            // If we have to emit two lines - take the second
            // one and stash it, then return the first one.
            // This way, the stashed line will be returned
            // on the next stream poll.
            (
                src,
                Emit::Two(
                    (line, initial_context, last_context),
                    (line_to_stash, context_to_stash, _),
                ),
            ) => {
                *this.stashed = Some((src.clone(), line_to_stash, context_to_stash));
                (src, line, initial_context, last_context)
            }
        };
        Some(val)
    }
}

/// Specifies the amount of lines to emit in response to a single input line.
/// We have to emit either one or two lines.
pub enum Emit<T> {
    /// Emit one line.
    One(T),
    /// Emit two lines, in the order they're specified.
    Two(T, T),
}

/// A helper enum
enum Decision {
    Continue,
    EndInclude,
    EndExclude,
}

impl<K, C> Logic<K, C>
where
    K: Hash + Eq + Clone,
{
    /// Handle line, if we have something to output - return it.
    pub fn handle_line(
        &mut self,
        src: K,
        line: Bytes,
        context: C,
    ) -> Option<(K, Emit<(Bytes, C, Option<C>)>)> {
        // Check if we already have the buffered data for the source.
        match self.buffers.entry(src) {
            Entry::Occupied(mut entry) => {
                let condition_matched = self.config.condition_pattern.is_match(line.as_ref());
                let decision = match (self.config.mode, condition_matched) {
                    // All consecutive lines matching this pattern are included in
                    // the group.
                    (Mode::ContinueThrough, true) => Decision::Continue,
                    (Mode::ContinueThrough, false) => Decision::EndExclude,
                    // All consecutive lines matching this pattern, plus one
                    // additional line, are included in the group.
                    (Mode::ContinuePast, true) => Decision::Continue,
                    (Mode::ContinuePast, false) => Decision::EndInclude,
                    // All consecutive lines not matching this pattern are included
                    // in the group.
                    (Mode::HaltBefore, true) => Decision::EndExclude,
                    (Mode::HaltBefore, false) => Decision::Continue,
                    // All consecutive lines, up to and including the first line
                    // matching this pattern, are included in the group.
                    (Mode::HaltWith, true) => Decision::EndInclude,
                    (Mode::HaltWith, false) => Decision::Continue,
                };

                match decision {
                    Decision::Continue => {
                        let buffered = entry.get_mut();
                        self.timeouts.reset(&buffered.0, self.config.timeout);
                        buffered.1.add_next_line(line, context);
                        None
                    }
                    Decision::EndInclude => {
                        let (src, (key, mut buffered)) = entry.remove_entry();
                        self.timeouts.remove(&key);
                        buffered.add_next_line(line, context);
                        Some((src, Emit::One(buffered.merge())))
                    }
                    Decision::EndExclude => {
                        let (src, (key, buffered)) = entry.remove_entry();
                        self.timeouts.remove(&key);
                        Some((src, Emit::Two(buffered.merge(), (line, context, None))))
                    }
                }
            }
            Entry::Vacant(entry) => {
                // This line is a candidate for buffering, or passing through.
                if self.config.start_pattern.is_match(line.as_ref()) {
                    // It was indeed a new line we need to filter.
                    // Set the timeout and buffer this line.
                    let key = self
                        .timeouts
                        .insert(entry.key().clone(), self.config.timeout);
                    entry.insert((key, Aggregate::new(line, context)));
                    None
                } else {
                    // It's just a regular line we don't really care about.
                    Some((entry.into_key(), Emit::One((line, context, None))))
                }
            }
        }
    }
}

struct Aggregate<C> {
    lines: Vec<Bytes>,
    initial_context: C,
    last_context: Option<C>,
}

impl<C> Aggregate<C> {
    fn new(first_line: Bytes, initial_context: C) -> Self {
        Self {
            lines: vec![first_line],
            initial_context,
            last_context: None,
        }
    }

    fn add_next_line(&mut self, line: Bytes, context: C) {
        self.last_context = Some(context);
        self.lines.push(line);
    }

    fn merge(self) -> (Bytes, C, Option<C>) {
        let capacity = self.lines.iter().map(|line| line.len() + 1).sum::<usize>() - 1;
        let mut bytes_mut = BytesMut::with_capacity(capacity);
        let mut first = true;
        for line in self.lines {
            if first {
                first = false;
            } else {
                bytes_mut.extend_from_slice(b"\n");
            }
            bytes_mut.extend_from_slice(&line);
        }
        (bytes_mut.freeze(), self.initial_context, self.last_context)
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use futures::SinkExt;
    use similar_asserts::assert_eq;
    use std::fmt::Write as _;

    use super::*;

    #[tokio::test]
    async fn mode_continue_through_1() {
        let lines = vec![
            "some usual line",
            "some other usual line",
            "first part",
            " second part",
            " last part",
            "another normal message",
            "finishing message",
            " last part of the incomplete finishing message",
        ];
        let config = Config {
            start_pattern: Regex::new("^[^\\s]").unwrap(),
            condition_pattern: Regex::new("^[\\s]+").unwrap(),
            mode: Mode::ContinueThrough,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            ("some usual line", 0, None),
            ("some other usual line", 1, None),
            (
                concat!("first part\n", " second part\n", " last part"),
                2,
                Some(4),
            ),
            ("another normal message", 5, None),
            (
                concat!(
                    "finishing message\n",
                    " last part of the incomplete finishing message"
                ),
                6,
                Some(7),
            ),
        ];
        run_and_assert(&lines, config, &expected).await;
    }

    #[tokio::test]
    async fn mode_continue_past_1() {
        let lines = vec![
            "some usual line",
            "some other usual line",
            "first part \\",
            "second part \\",
            "last part",
            "another normal message",
            "finishing message \\",
            "last part of the incomplete finishing message \\",
        ];
        let config = Config {
            start_pattern: Regex::new("\\\\$").unwrap(),
            condition_pattern: Regex::new("\\\\$").unwrap(),
            mode: Mode::ContinuePast,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            ("some usual line", 0, None),
            ("some other usual line", 1, None),
            (
                concat!("first part \\\n", "second part \\\n", "last part"),
                2,
                Some(4),
            ),
            ("another normal message", 5, None),
            (
                concat!(
                    "finishing message \\\n",
                    "last part of the incomplete finishing message \\"
                ),
                6,
                Some(7),
            ),
        ];
        run_and_assert(&lines, config, &expected).await;
    }

    #[tokio::test]
    async fn mode_halt_before_1() {
        let lines = vec![
            "INFO some usual line",
            "INFO some other usual line",
            "INFO first part",
            "second part",
            "last part",
            "ERROR another normal message",
            "ERROR finishing message",
            "last part of the incomplete finishing message",
        ];
        let config = Config {
            start_pattern: Regex::new("").unwrap(),
            condition_pattern: Regex::new("^(INFO|ERROR) ").unwrap(),
            mode: Mode::HaltBefore,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            ("INFO some usual line", 0, None),
            ("INFO some other usual line", 1, None),
            (
                concat!("INFO first part\n", "second part\n", "last part"),
                2,
                Some(4),
            ),
            ("ERROR another normal message", 5, None),
            (
                concat!(
                    "ERROR finishing message\n",
                    "last part of the incomplete finishing message"
                ),
                6,
                Some(7),
            ),
        ];
        run_and_assert(&lines, config, &expected).await;
    }

    #[tokio::test]
    async fn mode_halt_with_1() {
        let lines = vec![
            "some usual line;",
            "some other usual line;",
            "first part",
            "second part",
            "last part;",
            "another normal message;",
            "finishing message",
            "last part of the incomplete finishing message",
        ];
        let config = Config {
            start_pattern: Regex::new("[^;]$").unwrap(),
            condition_pattern: Regex::new(";$").unwrap(),
            mode: Mode::HaltWith,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            ("some usual line;", 0, None),
            ("some other usual line;", 1, None),
            (
                concat!("first part\n", "second part\n", "last part;"),
                2,
                Some(4),
            ),
            ("another normal message;", 5, None),
            (
                concat!(
                    "finishing message\n",
                    "last part of the incomplete finishing message"
                ),
                6,
                Some(7),
            ),
        ];
        run_and_assert(&lines, config, &expected).await;
    }

    #[tokio::test]
    async fn use_case_java_exception() {
        let lines = vec![
            "java.lang.Exception",
            "    at com.foo.bar(bar.java:123)",
            "    at com.foo.baz(baz.java:456)",
        ];
        let config = Config {
            start_pattern: Regex::new("^[^\\s]").unwrap(),
            condition_pattern: Regex::new("^[\\s]+at").unwrap(),
            mode: Mode::ContinueThrough,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![(
            concat!(
                "java.lang.Exception\n",
                "    at com.foo.bar(bar.java:123)\n",
                "    at com.foo.baz(baz.java:456)",
            ),
            0,
            Some(2),
        )];
        run_and_assert(&lines, config, &expected).await;
    }

    #[tokio::test]
    async fn use_case_ruby_exception() {
        let lines = vec![
            "foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)",
            "\tfrom foobar.rb:6:in `bar'",
            "\tfrom foobar.rb:2:in `foo'",
            "\tfrom foobar.rb:9:in `<main>'",
        ];
        let config = Config {
            start_pattern: Regex::new("^[^\\s]").unwrap(),
            condition_pattern: Regex::new("^[\\s]+from").unwrap(),
            mode: Mode::ContinueThrough,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![(
            concat!(
                "foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)\n",
                "\tfrom foobar.rb:6:in `bar'\n",
                "\tfrom foobar.rb:2:in `foo'\n",
                "\tfrom foobar.rb:9:in `<main>'",
            ),
            0,
            Some(3),
        )];
        run_and_assert(&lines, config, &expected).await;
    }

    /// https://github.com/vectordotdev/vector/issues/3237
    #[tokio::test]
    async fn two_lines_emit_with_continue_through() {
        let lines = vec![
            "not merged 1", // will NOT be stashed, but passed-through
            " merged 1",
            " merged 2",
            "not merged 2", // will be stashed
            " merged 3",
            " merged 4",
            "not merged 3", // will be stashed
            "not merged 4", // will NOT be stashed, but passed-through
            " merged 5",
            "not merged 5", // will be stashed
            " merged 6",
            " merged 7",
            " merged 8",
            "not merged 6", // will be stashed
        ];
        let config = Config {
            start_pattern: Regex::new("^\\s").unwrap(),
            condition_pattern: Regex::new("^\\s").unwrap(),
            mode: Mode::ContinueThrough,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            ("not merged 1", 0, None),
            (" merged 1\n merged 2", 1, Some(2)),
            ("not merged 2", 3, None),
            (" merged 3\n merged 4", 4, Some(5)),
            ("not merged 3", 6, None),
            ("not merged 4", 7, None),
            (" merged 5", 8, None),
            ("not merged 5", 9, None),
            (" merged 6\n merged 7\n merged 8", 10, Some(12)),
            ("not merged 6", 13, None),
        ];
        run_and_assert(&lines, config, &expected).await;
    }

    #[tokio::test]
    async fn two_lines_emit_with_halt_before() {
        let lines = vec![
            "part 0.1",
            "part 0.2",
            "START msg 1", // will be stashed
            "part 1.1",
            "part 1.2",
            "START msg 2", // will be stashed
            "START msg 3", // will be stashed
            "part 3.1",
            "START msg 4", // will be stashed
            "part 4.1",
            "part 4.2",
            "part 4.3",
            "START msg 5", // will be stashed
        ];
        let config = Config {
            start_pattern: Regex::new("").unwrap(),
            condition_pattern: Regex::new("^START ").unwrap(),
            mode: Mode::HaltBefore,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            ("part 0.1\npart 0.2", 0, Some(1)),
            ("START msg 1\npart 1.1\npart 1.2", 2, Some(4)),
            ("START msg 2", 5, None),
            ("START msg 3\npart 3.1", 6, Some(7)),
            ("START msg 4\npart 4.1\npart 4.2\npart 4.3", 8, Some(11)),
            ("START msg 5", 12, None),
        ];
        run_and_assert(&lines, config, &expected).await;
    }

    #[tokio::test]
    async fn legacy() {
        let lines = vec![
            "INFO some usual line",
            "INFO some other usual line",
            "INFO first part",
            "second part",
            "last part",
            "ERROR another normal message",
            "ERROR finishing message",
            "last part of the incomplete finishing message",
        ];
        let expected = vec![
            ("INFO some usual line", 0, None),
            ("INFO some other usual line", 1, None),
            (
                concat!("INFO first part\n", "second part\n", "last part"),
                2,
                Some(4),
            ),
            ("ERROR another normal message", 5, None),
            (
                concat!(
                    "ERROR finishing message\n",
                    "last part of the incomplete finishing message"
                ),
                6,
                Some(7),
            ),
        ];

        let stream = stream_from_lines(&lines);
        let line_agg = LineAgg::new(
            stream,
            Logic::new(Config::for_legacy(
                Regex::new("^(INFO|ERROR)").unwrap(), // example from the docs
                10,
            )),
        );
        let results = line_agg.collect().await;
        assert_results(results, &expected);
    }

    #[tokio::test]
    async fn timeout_resets_on_new_line() {
        // Tests if multiline aggregation updates
        // it's timeout every time it get's a new line.
        // To test this we are emitting a single large
        // multiline but drip feeding it into the aggregator
        // with 1ms delay.

        let n: usize = 1000;
        let mut lines = vec![
            "START msg 1".to_string(), // will be stashed
        ];
        for i in 0..n {
            lines.push(format!("line {}", i));
        }
        let config = Config {
            start_pattern: Regex::new("").unwrap(),
            condition_pattern: Regex::new("^START ").unwrap(),
            mode: Mode::HaltBefore,
            timeout: Duration::from_millis(10),
        };

        let mut expected = "START msg 1".to_string();
        for i in 0..n {
            write!(expected, "\nline {}", i).expect("write to String never fails");
        }

        let (mut send, recv) = futures::channel::mpsc::unbounded();

        let logic = Logic::new(config);
        let line_agg = LineAgg::new(recv, logic);
        let results = tokio::spawn(line_agg.collect());

        for (index, line) in lines.iter().enumerate() {
            let data = (
                "test.log".to_owned(),
                Bytes::copy_from_slice(line.as_bytes()),
                index,
            );
            send.send(data).await.unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        drop(send);

        assert_results(
            results.await.unwrap(),
            &[(expected.as_str(), 0, Some(lines.len() - 1))],
        );
    }

    // Test helpers.

    /// Private type alias to be more expressive in the internal implementation.
    type Filename = String;

    fn stream_from_lines<'a>(
        lines: &'a [&'static str],
    ) -> impl Stream<Item = (Filename, Bytes, usize)> + 'a {
        futures::stream::iter(lines.iter().enumerate().map(|(index, line)| {
            (
                "test.log".to_owned(),
                Bytes::from_static(line.as_bytes()),
                index,
            )
        }))
    }

    /// Compare actual output to expected; expected is a list of the expected strings and context
    fn assert_results(
        actual: Vec<(Filename, Bytes, usize, Option<usize>)>,
        expected: &[(&str, usize, Option<usize>)],
    ) {
        let expected_mapped: Vec<(Filename, Bytes, usize, Option<usize>)> = expected
            .iter()
            .map(|(line, context, last_context)| {
                (
                    "test.log".to_owned(),
                    Bytes::copy_from_slice(line.as_bytes()),
                    *context,
                    *last_context,
                )
            })
            .collect();

        assert_eq!(
            actual, expected_mapped,
            "actual on the left, expected on the right",
        );
    }

    async fn run_and_assert(
        lines: &[&'static str],
        config: Config,
        expected: &[(&'static str, usize, Option<usize>)],
    ) {
        let stream = stream_from_lines(lines);
        let logic = Logic::new(config);
        let line_agg = LineAgg::new(stream, logic);
        let results = line_agg.collect().await;
        assert_results(results, expected);
    }
}
