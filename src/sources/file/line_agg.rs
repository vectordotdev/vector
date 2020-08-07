#![deny(missing_docs)]

use bytes05::{Bytes, BytesMut};
use futures::{Stream, StreamExt};
use pin_project::pin_project;
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::Entry, HashMap, VecDeque};
use std::hash::Hash;
use std::time::Duration;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::time::DelayQueue;

/// The mode of operation of the line aggregator.
#[derive(Debug, Hash, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// All consecutive lines matching this pattern are included in the group.
    /// The first line (the line that matched the start pattern) does not need
    /// to match the `ContinueThrough` pattern.
    /// This is useful in cases such as a Java stack trace, where some indicator
    /// in the line (such as leading whitespace) indicates that it is an
    /// extension of the preceeding line.
    ContinueThrough,

    /// All consecutive lines matching this pattern, plus one additional line,
    /// are included in the group.
    /// This is useful in cases where a log message ends with a continuation
    /// marker, such as a backslash, indicating that the following line is part
    /// of the same message.
    ContinuePast,

    /// All consecutive lines not matching this pattern are included in the
    /// group.
    /// This is useful where a log line contains a marker indicating that it
    /// begins a new message.
    HaltBefore,

    /// All consecutive lines, up to and including the first line matching this
    /// pattern, are included in the group.
    /// This is useful where a log line ends with a termination marker, such as
    /// a semicolon.
    HaltWith,
}

/// Configuration parameters of the line aggregator.
#[derive(Debug, Clone)]
pub struct Config {
    /// Start pattern to look for as a beginning of the message.
    pub start_pattern: Regex,
    /// Condition pattern to look for. Exact behavior is configured via `mode`.
    pub condition_pattern: Regex,
    /// Mode of operation, specifies how the condition pattern is interpreted.
    pub mode: Mode,
    /// The maximum time to wait for the continuation. Once this timeout is
    /// reached, the buffered message is guaraneed to be flushed, even if
    /// incomplete.
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
pub struct LineAgg<T, K> {
    /// The stream from which we read the lines.
    #[pin]
    inner: T,

    /// The core line aggregation logic.
    logic: Logic<K>,

    /// Stashed lines. When line aggreation results in more than one line being
    /// emitted, we have to stash lines and return them into the stream after
    /// that before doing any other work.
    stashed: Option<(Bytes, K)>,

    /// Draining queue. We switch to draining mode when we get `None` from
    /// the inner stream. In this mode we stop polling `inner` for new lines
    /// and just flush all the buffered data.
    draining: Option<Vec<(Bytes, K)>>,

    /// A queue of keys with expired timeouts.
    expired: VecDeque<K>,
}

/// Core line aggregation logic.
///
/// Encapsulates the essential state and the core logic for the line
/// aggregation algorithm.
pub struct Logic<K> {
    /// Configuration parameters to use.
    config: Config,

    /// Line per key.
    /// Key is usually a filename or other line source identifier.
    buffers: HashMap<K, BytesMut>,

    /// A queue of key timeouts.
    timeouts: DelayQueue<K>,
}

impl<K> Logic<K> {
    /// Create a new `Logic` using the specified `Config`.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            buffers: HashMap::new(),
            timeouts: DelayQueue::new(),
        }
    }
}

impl<T, K> LineAgg<T, K>
where
    T: Stream<Item = (Bytes, K)> + Unpin,
    K: Hash + Eq + Clone,
{
    /// Create a new `LineAgg` using the specified `inner` stream and
    /// preconfigured `logic`.
    pub fn new(inner: T, logic: Logic<K>) -> Self {
        Self {
            inner,
            logic,
            draining: None,
            stashed: None,
            expired: VecDeque::new(),
        }
    }
}

impl<T, K> Stream for LineAgg<T, K>
where
    T: Stream<Item = (Bytes, K)> + Unpin,
    K: Hash + Eq + Clone,
{
    /// `Bytes` - the line data; `K` - file name, or other line source.
    type Item = (Bytes, K);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            // If we have a stashed line, process it before doing anything else.
            if let Some((line, src)) = this.stashed.take() {
                // Handle the stashed line. If the handler gave us something -
                // return it, otherwise restart the loop iteration to start
                // anew. Handler could've stashed another value, continuing to
                // the new loop iteration handles that.
                if let Some(val) = Self::handle_line_and_stashing(&mut this, line, src) {
                    return Poll::Ready(Some(val));
                }
                continue;
            }

            // If we're in draining mode, short circut here.
            if let Some(to_drain) = &mut this.draining {
                if let Some(val) = to_drain.pop() {
                    return Poll::Ready(Some(val));
                } else {
                    return Poll::Ready(None);
                }
            }

            // Check for keys that have hit their timeout.
            while let Poll::Ready(Some(Ok(expired_key))) = this.logic.timeouts.poll_expired(cx) {
                this.expired.push_back(expired_key.into_inner());
            }

            match this.inner.poll_next_unpin(cx) {
                Poll::Ready(Some((line, src))) => {
                    // Handle the incoming line we got from `inner`. If the
                    // handler gave us something - return it, otherwise continue
                    // with the flow.
                    if let Some(val) = Self::handle_line_and_stashing(&mut this, line, src) {
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
                            .map(|(k, v)| (v.into(), k))
                            .collect(),
                    );
                }
                Poll::Pending => {
                    // We didn't get any lines from `inner`, so we just give
                    // a line from the expired lines queue.
                    if let Some(key) = this.expired.pop_front() {
                        if let Some(buffered) = this.logic.buffers.remove(&key) {
                            return Poll::Ready(Some((buffered.freeze(), key)));
                        }
                    }

                    return Poll::Pending;
                }
            };
        }
    }
}

impl<T, K> LineAgg<T, K>
where
    T: Stream<Item = (Bytes, K)> + Unpin,
    K: Hash + Eq + Clone,
{
    /// Handle line and do stashing of extra emitted lines.
    /// Requires that the `stashed` item is empty (i.e. entry is vacant). This
    /// invariant has to be taken care of by the caller.
    fn handle_line_and_stashing(
        this: &mut LineAggProj<'_, T, K>,
        line: Bytes,
        src: K,
    ) -> Option<(Bytes, K)> {
        // Stashed line is always consumed at the start of the `poll`
        // loop before entering this line processing logic. If it's
        // non-empty here - it's a bug.
        debug_assert!(this.stashed.is_none());
        let val = this.logic.handle_line(line, src)?;
        let val = match val {
            // If we have to emit just one line - that's easy,
            // we just return it.
            (Emit::One(line), src) => (line, src),
            // If we have to emit two lines - take the second
            // one and stash it, then return the first one.
            // This way, the stashed line will be returned
            // on the next stream poll.
            (Emit::Two(line, to_stash), src) => {
                *this.stashed = Some((to_stash, src.clone()));
                (line, src)
            }
        };
        Some(val)
    }
}

/// Specifies the amount of lines to emit in response to a single input line.
/// We have to emit either one or two lines.
pub enum Emit {
    /// Emit one line.
    One(Bytes),
    /// Emit two lines, in the order they're specified.
    Two(Bytes, Bytes),
}

impl<K> Logic<K>
where
    K: Hash + Eq + Clone,
{
    /// Handle line, if we have something to output - return it.
    pub fn handle_line(&mut self, line: Bytes, src: K) -> Option<(Emit, K)> {
        // Check if we already have the buffered data for the source.
        match self.buffers.entry(src) {
            Entry::Occupied(mut entry) => {
                let condition_matched = self.config.condition_pattern.is_match(line.as_ref());
                match self.config.mode {
                    // All consecutive lines matching this pattern are included in
                    // the group.
                    Mode::ContinueThrough => {
                        if condition_matched {
                            let buffered = entry.get_mut();
                            add_next_line(buffered, line);
                            None
                        } else {
                            let (src, buffered) = entry.remove_entry();
                            Some((Emit::Two(buffered.freeze(), line), src))
                        }
                    }
                    // All consecutive lines matching this pattern, plus one
                    // additional line, are included in the group.
                    Mode::ContinuePast => {
                        if condition_matched {
                            let buffered = entry.get_mut();
                            add_next_line(buffered, line);
                            None
                        } else {
                            let (src, mut buffered) = entry.remove_entry();
                            add_next_line(&mut buffered, line);
                            Some((Emit::One(buffered.freeze()), src))
                        }
                    }
                    // All consecutive lines not matching this pattern are included
                    // in the group.
                    Mode::HaltBefore => {
                        if condition_matched {
                            let (src, buffered) = entry.remove_entry();
                            Some((Emit::Two(buffered.freeze(), line), src))
                        } else {
                            let buffered = entry.get_mut();
                            add_next_line(buffered, line);
                            None
                        }
                    }
                    // All consecutive lines, up to and including the first line
                    // matching this pattern, are included in the group.
                    Mode::HaltWith => {
                        if condition_matched {
                            let (src, mut buffered) = entry.remove_entry();
                            add_next_line(&mut buffered, line);
                            Some((Emit::One(buffered.freeze()), src))
                        } else {
                            let buffered = entry.get_mut();
                            add_next_line(buffered, line);
                            None
                        }
                    }
                }
            }
            Entry::Vacant(entry) => {
                // This line is a candidate for buffering, or passing through.
                if self.config.start_pattern.is_match(line.as_ref()) {
                    // It was indeed a new line we need to filter.
                    // Set the timeout and buffer this line.
                    self.timeouts
                        .insert(entry.key().clone(), self.config.timeout);
                    entry.insert(line.as_ref().into());
                    None
                } else {
                    // It's just a regular line we don't really care about.
                    Some((Emit::One(line), entry.into_key()))
                }
            }
        }
    }
}

fn add_next_line(buffered: &mut BytesMut, line: Bytes) {
    buffered.extend_from_slice(b"\n");
    buffered.extend_from_slice(&line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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
            "some usual line",
            "some other usual line",
            concat!("first part\n", " second part\n", " last part"),
            "another normal message",
            concat!(
                "finishing message\n",
                " last part of the incomplete finishing message"
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
            "some usual line",
            "some other usual line",
            concat!("first part \\\n", "second part \\\n", "last part"),
            "another normal message",
            concat!(
                "finishing message \\\n",
                "last part of the incomplete finishing message \\"
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
            "INFO some usual line",
            "INFO some other usual line",
            concat!("INFO first part\n", "second part\n", "last part"),
            "ERROR another normal message",
            concat!(
                "ERROR finishing message\n",
                "last part of the incomplete finishing message"
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
            "some usual line;",
            "some other usual line;",
            concat!("first part\n", "second part\n", "last part;"),
            "another normal message;",
            concat!(
                "finishing message\n",
                "last part of the incomplete finishing message"
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
        let expected = vec![concat!(
            "java.lang.Exception\n",
            "    at com.foo.bar(bar.java:123)\n",
            "    at com.foo.baz(baz.java:456)",
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
        let expected = vec![concat!(
            "foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)\n",
            "\tfrom foobar.rb:6:in `bar'\n",
            "\tfrom foobar.rb:2:in `foo'\n",
            "\tfrom foobar.rb:9:in `<main>'",
        )];
        run_and_assert(&lines, config, &expected).await;
    }

    /// https://github.com/timberio/vector/issues/3237
    #[tokio::test]
    async fn two_lines_emit_with_continue_through() {
        let lines = vec![
            "not merged 1", // will NOT be stashed, but passthroughed
            " merged 1",
            " merged 2",
            "not merged 2", // will be stashed
            " merged 3",
            " merged 4",
            "not merged 3", // will be stashed
            "not merged 4", // will NOT be stashed, but passthroughed
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
            "not merged 1",
            " merged 1\n merged 2",
            "not merged 2",
            " merged 3\n merged 4",
            "not merged 3",
            "not merged 4",
            " merged 5",
            "not merged 5",
            " merged 6\n merged 7\n merged 8",
            "not merged 6",
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
            "part 0.1\npart 0.2",
            "START msg 1\npart 1.1\npart 1.2",
            "START msg 2",
            "START msg 3\npart 3.1",
            "START msg 4\npart 4.1\npart 4.2\npart 4.3",
            "START msg 5",
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
            "INFO some usual line",
            "INFO some other usual line",
            concat!("INFO first part\n", "second part\n", "last part"),
            "ERROR another normal message",
            concat!(
                "ERROR finishing message\n",
                "last part of the incomplete finishing message"
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

    // Test helpers.

    /// Private type alias to be more expressive in the internal implementation.
    type Filename = String;

    fn stream_from_lines<'a>(
        lines: &'a [&'static str],
    ) -> impl Stream<Item = (Bytes, Filename)> + 'a {
        futures::stream::iter(
            lines
                .iter()
                .map(|line| (Bytes::from_static(line.as_bytes()), "test.log".to_owned())),
        )
    }

    fn assert_results(actual: Vec<(Bytes, Filename)>, expected: &[&'static str]) {
        let expected_mapped: Vec<(Bytes, Filename)> = expected
            .iter()
            .map(|line| (Bytes::from_static(line.as_bytes()), "test.log".to_owned()))
            .collect();

        assert_eq!(
            actual, expected_mapped,
            "actual on the left, expected on the right",
        );
    }

    async fn run_and_assert(lines: &[&'static str], config: Config, expected: &[&'static str]) {
        let stream = stream_from_lines(lines);
        let logic = Logic::new(config);
        let line_agg = LineAgg::new(stream, logic);
        let results = line_agg.collect().await;
        assert_results(results, expected);
    }
}
