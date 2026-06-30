use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
    time::Duration,
};

use futures_util::Stream;
use pin_project::pin_project;
use tokio::time::{Instant, Sleep, sleep};
use vector_lib::{buffers::BufferUsageObserver, configurable::configurable_component};

const MIN_SLEEP: Duration = Duration::from_millis(1);

/// Read-only buffer usage source for drain shaping.
pub trait BufferUsageSource: Send + Sync {
    /// Gets cumulative ingress volume as `(events, bytes)`.
    fn received(&self) -> (u64, u64);

    /// Gets current buffer occupancy as `(events, bytes)`.
    fn occupancy(&self) -> (u64, u64);

    /// Gets configured maximum buffer size as `(events, bytes)`.
    fn max_size(&self) -> (u64, u64);
}

impl BufferUsageSource for BufferUsageObserver {
    fn received(&self) -> (u64, u64) {
        self.received()
    }

    fn occupancy(&self) -> (u64, u64) {
        self.occupancy()
    }

    fn max_size(&self) -> (u64, u64) {
        self.max_size()
    }
}

/// Backlog-aware output pacing.
///
/// When enabled, Vector estimates the byte rate entering the sink buffer and caps original
/// output submissions to `max(estimated_input_rate * factor, min_drain_bytes_per_sec)` after
/// enough buffered bytes accumulate.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct DrainShapingConfig {
    /// Enables backlog-aware output pacing.
    pub enabled: bool,

    /// Multiplier applied to the estimated input byte rate while the buffer has a backlog.
    pub factor: f64,

    /// Minimum drain budget in bytes per second while pacing is active.
    ///
    /// Set this above the expected sustained input rate to ensure a full disk buffer can drain
    /// after restart, when no recent input-rate history exists.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub min_drain_bytes_per_sec: u64,

    /// Optional maximum number of original requests submitted per second.
    ///
    /// This caps submissions before request retries. It does not debit retry attempts made by
    /// the request service.
    pub max_requests_per_sec: Option<f64>,

    /// Exponential moving average alpha used for input byte-rate estimation.
    pub ewma_alpha: f64,

    /// Buffer usage sample interval in seconds.
    pub sample_interval_secs: f64,

    /// Minimum buffered bytes before pacing engages.
    ///
    /// Below this absolute backlog threshold, the sink submits requests without delay.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub engage_min_bytes: u64,

    /// Buffer fill ratio above which input-rate estimation freezes.
    ///
    /// Freezing prevents a full `when_full = "block"` buffer from coupling the observed input
    /// rate to the shaped output rate.
    pub saturation_high_mark: f64,

    /// Buffer fill ratio below which frozen input-rate estimation resumes.
    pub saturation_low_mark: f64,

    /// Recent input-rate window, in seconds, used to seed the frozen estimate.
    pub freeze_window_secs: f64,

    /// Additional byte burst budget allowed above steady-state pacing.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub burst_bytes: u64,
}

impl Default for DrainShapingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            factor: 1.1,
            min_drain_bytes_per_sec: 0,
            max_requests_per_sec: None,
            ewma_alpha: 0.1,
            sample_interval_secs: 0.5,
            engage_min_bytes: 1_048_576,
            saturation_high_mark: 0.9,
            saturation_low_mark: 0.75,
            freeze_window_secs: 30.0,
            burst_bytes: 0,
        }
    }
}

impl DrainShapingConfig {
    /// Validates drain shaping settings.
    ///
    /// # Errors
    ///
    /// Returns an error when enabled settings cannot guarantee bounded, lossless pacing.
    pub fn validate(&self, static_partition_key: bool) -> crate::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if !static_partition_key {
            return Err("drain_shaping requires a static partition key".into());
        }

        if !self.factor.is_finite() || self.factor < 1.0 {
            return Err("drain_shaping.factor must be finite and >= 1".into());
        }

        if self.min_drain_bytes_per_sec == 0 {
            return Err("drain_shaping.min_drain_bytes_per_sec must be greater than 0".into());
        }

        if !self.ewma_alpha.is_finite() || self.ewma_alpha <= 0.0 || self.ewma_alpha > 1.0 {
            return Err("drain_shaping.ewma_alpha must be finite and in (0, 1]".into());
        }

        if !self.sample_interval_secs.is_finite() || self.sample_interval_secs <= 0.0 {
            return Err("drain_shaping.sample_interval_secs must be finite and > 0".into());
        }

        if !self.freeze_window_secs.is_finite() || self.freeze_window_secs <= 0.0 {
            return Err("drain_shaping.freeze_window_secs must be finite and > 0".into());
        }

        if !self.saturation_low_mark.is_finite()
            || !self.saturation_high_mark.is_finite()
            || self.saturation_low_mark < 0.0
            || self.saturation_low_mark >= self.saturation_high_mark
            || self.saturation_high_mark > 1.0
        {
            return Err("drain_shaping saturation marks must satisfy 0 <= low < high <= 1".into());
        }

        if let Some(max_requests_per_sec) = self.max_requests_per_sec
            && (!max_requests_per_sec.is_finite() || max_requests_per_sec <= 0.0)
        {
            return Err("drain_shaping.max_requests_per_sec must be finite and > 0".into());
        }

        Ok(())
    }
}

/// Stream adapter that paces sink output based on observed buffer ingress and occupancy.
#[pin_project]
pub struct DrainShaper<S, F>
where
    S: Stream,
{
    #[pin]
    inner: S,
    size_of: F,
    source: Option<Arc<dyn BufferUsageSource>>,
    config: DrainShapingConfig,
    pending: Option<S::Item>,
    last_received_bytes: u64,
    last_sample: Instant,
    input_ewma: f64,
    frozen: bool,
    freeze_anchor: f64,
    recent: VecDeque<(Instant, f64)>,
    byte_tokens: f64,
    request_tokens: f64,
    last_refill: Instant,
    #[pin]
    delay: Option<Sleep>,
}

impl<S, F> DrainShaper<S, F>
where
    S: Stream,
{
    /// Creates a drain-shaping stream adapter.
    pub fn new(
        inner: S,
        source: Option<Arc<dyn BufferUsageSource>>,
        config: DrainShapingConfig,
        size_of: F,
    ) -> Self {
        let now = Instant::now();
        let last_received_bytes = source
            .as_ref()
            .map(|source| source.received().1)
            .unwrap_or_default();

        Self {
            inner,
            size_of,
            source,
            config,
            pending: None,
            last_received_bytes,
            last_sample: now,
            input_ewma: 0.0,
            frozen: false,
            freeze_anchor: 0.0,
            recent: VecDeque::new(),
            byte_tokens: config.burst_bytes as f64,
            request_tokens: if config.max_requests_per_sec.is_some() {
                1.0
            } else {
                0.0
            },
            last_refill: now,
            delay: None,
        }
    }
}

impl<S, F> Stream for DrainShaper<S, F>
where
    S: Stream,
    F: Fn(&S::Item) -> usize,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if !this.config.enabled || this.source.is_none() {
            return this.inner.poll_next(cx);
        }

        let source = Arc::clone(this.source.as_ref().expect("source checked above"));

        loop {
            if this.pending.is_none() {
                match ready!(this.inner.as_mut().poll_next(cx)) {
                    Some(item) => *this.pending = Some(item),
                    None => return Poll::Ready(None),
                }
            }

            if let Some(delay) = this.delay.as_mut().as_pin_mut() {
                ready!(delay.poll(cx));
                this.delay.set(None);
            }

            let now = Instant::now();
            update_estimator(
                *this.config,
                &*source,
                EstimatorState {
                    last_received_bytes: this.last_received_bytes,
                    last_sample: this.last_sample,
                    input_ewma: this.input_ewma,
                    frozen: this.frozen,
                    freeze_anchor: this.freeze_anchor,
                    recent: this.recent,
                },
                now,
            );

            if source.occupancy().1 < this.config.engage_min_bytes {
                return Poll::Ready(this.pending.take());
            }

            let rate = effective_rate(
                *this.config,
                *this.input_ewma,
                *this.frozen,
                *this.freeze_anchor,
            );
            refill(
                *this.config,
                this.byte_tokens,
                this.request_tokens,
                this.last_refill,
                rate,
                now,
            );

            if *this.byte_tokens >= 0.0 && request_ok(*this.config, *this.request_tokens) {
                let item = this.pending.take().expect("pending item checked above");
                *this.byte_tokens -= (this.size_of)(&item) as f64;
                debit_request(*this.config, this.request_tokens);
                return Poll::Ready(Some(item));
            }

            this.delay.set(Some(sleep(time_until_emittable(
                *this.config,
                *this.byte_tokens,
                *this.request_tokens,
                rate,
            ))));
        }
    }
}

/// Extension trait for applying drain shaping to streams.
pub trait DrainShapingExt: Stream + Sized {
    /// Wraps the stream in a [`DrainShaper`].
    fn drain_shaping<F>(
        self,
        source: Option<Arc<dyn BufferUsageSource>>,
        config: DrainShapingConfig,
        size_of: F,
    ) -> DrainShaper<Self, F>
    where
        F: Fn(&Self::Item) -> usize,
    {
        DrainShaper::new(self, source, config, size_of)
    }
}

impl<S> DrainShapingExt for S where S: Stream + Sized {}

struct EstimatorState<'a> {
    last_received_bytes: &'a mut u64,
    last_sample: &'a mut Instant,
    input_ewma: &'a mut f64,
    frozen: &'a mut bool,
    freeze_anchor: &'a mut f64,
    recent: &'a mut VecDeque<(Instant, f64)>,
}

fn update_estimator(
    config: DrainShapingConfig,
    source: &dyn BufferUsageSource,
    state: EstimatorState<'_>,
    now: Instant,
) {
    let elapsed = now.duration_since(*state.last_sample);
    if elapsed < Duration::from_secs_f64(config.sample_interval_secs) {
        return;
    }

    let received_bytes = source.received().1;
    let rate =
        received_bytes.saturating_sub(*state.last_received_bytes) as f64 / elapsed.as_secs_f64();
    *state.last_received_bytes = received_bytes;
    *state.last_sample = now;

    trim_recent(
        state.recent,
        now,
        Duration::from_secs_f64(config.freeze_window_secs),
    );

    let was_frozen = *state.frozen;
    let fill_ratio = fill_ratio(source);
    if *state.frozen {
        if fill_ratio < config.saturation_low_mark {
            *state.frozen = false;
        }
    } else if fill_ratio >= config.saturation_high_mark {
        *state.frozen = true;
        *state.freeze_anchor = recent_window_max(state.recent, *state.input_ewma);
    }

    if *state.frozen {
        return;
    }

    if was_frozen {
        *state.input_ewma = rate;
    } else {
        *state.input_ewma =
            (config.ewma_alpha * rate) + ((1.0 - config.ewma_alpha) * *state.input_ewma);
    }
    state.recent.push_back((now, rate));
}

fn fill_ratio(source: &dyn BufferUsageSource) -> f64 {
    let max_bytes = source.max_size().1;
    if max_bytes == 0 {
        return 0.0;
    }

    source.occupancy().1 as f64 / max_bytes as f64
}

fn trim_recent(recent: &mut VecDeque<(Instant, f64)>, now: Instant, window: Duration) {
    while let Some((sampled_at, _)) = recent.front() {
        if now.duration_since(*sampled_at) <= window {
            break;
        }

        recent.pop_front();
    }
}

fn recent_window_max(recent: &VecDeque<(Instant, f64)>, input_ewma: f64) -> f64 {
    recent
        .iter()
        .map(|(_, rate)| *rate)
        .fold(input_ewma, f64::max)
}

fn effective_rate(
    config: DrainShapingConfig,
    input_ewma: f64,
    frozen: bool,
    freeze_anchor: f64,
) -> f64 {
    let estimate = if frozen { freeze_anchor } else { input_ewma };
    (estimate * config.factor).max(config.min_drain_bytes_per_sec as f64)
}

fn refill(
    config: DrainShapingConfig,
    byte_tokens: &mut f64,
    request_tokens: &mut f64,
    last_refill: &mut Instant,
    rate: f64,
    now: Instant,
) {
    let elapsed = now.duration_since(*last_refill).as_secs_f64();
    if elapsed == 0.0 {
        return;
    }

    *byte_tokens = (*byte_tokens + (rate * elapsed)).min(config.burst_bytes as f64);
    if let Some(max_requests_per_sec) = config.max_requests_per_sec {
        *request_tokens = (*request_tokens + (max_requests_per_sec * elapsed)).min(1.0);
    }
    *last_refill = now;
}

fn request_ok(config: DrainShapingConfig, request_tokens: f64) -> bool {
    match config.max_requests_per_sec {
        Some(_) => request_tokens >= 1.0,
        None => true,
    }
}

fn debit_request(config: DrainShapingConfig, request_tokens: &mut f64) {
    if config.max_requests_per_sec.is_some() {
        *request_tokens -= 1.0;
    }
}

fn time_until_emittable(
    config: DrainShapingConfig,
    byte_tokens: f64,
    request_tokens: f64,
    rate: f64,
) -> Duration {
    let byte_wait = if byte_tokens < 0.0 {
        (-byte_tokens) / rate.max(f64::MIN_POSITIVE)
    } else {
        0.0
    };
    let request_wait = match config.max_requests_per_sec {
        Some(max_requests_per_sec) if request_tokens < 1.0 => {
            (1.0 - request_tokens) / max_requests_per_sec
        }
        _ => 0.0,
    };

    Duration::from_secs_f64(byte_wait.max(request_wait)).max(MIN_SLEEP)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering::Relaxed},
        },
        task::Poll,
        time::Duration,
    };

    use futures::{StreamExt, stream};
    use tokio::time::{self, Instant};
    use vector_lib::buffers::BufferUsageObserver;

    use super::{BufferUsageSource, DrainShapingConfig, DrainShapingExt};

    #[derive(Debug)]
    struct TestBufferUsageSource {
        received_bytes: AtomicU64,
        occupancy_bytes: AtomicU64,
        max_bytes: AtomicU64,
    }

    impl TestBufferUsageSource {
        fn new(max_bytes: u64) -> Arc<Self> {
            Arc::new(Self {
                received_bytes: AtomicU64::new(0),
                occupancy_bytes: AtomicU64::new(0),
                max_bytes: AtomicU64::new(max_bytes),
            })
        }

        fn set_received_bytes(&self, bytes: u64) {
            self.received_bytes.store(bytes, Relaxed);
        }

        fn set_occupancy_bytes(&self, bytes: u64) {
            self.occupancy_bytes.store(bytes, Relaxed);
        }
    }

    impl BufferUsageSource for TestBufferUsageSource {
        fn received(&self) -> (u64, u64) {
            (0, self.received_bytes.load(Relaxed))
        }

        fn occupancy(&self) -> (u64, u64) {
            (0, self.occupancy_bytes.load(Relaxed))
        }

        fn max_size(&self) -> (u64, u64) {
            (0, self.max_bytes.load(Relaxed))
        }
    }

    fn valid_enabled_config() -> DrainShapingConfig {
        DrainShapingConfig {
            enabled: true,
            factor: 1.1,
            min_drain_bytes_per_sec: 1024,
            max_requests_per_sec: None,
            ewma_alpha: 0.1,
            sample_interval_secs: 0.5,
            engage_min_bytes: 1,
            saturation_high_mark: 0.9,
            saturation_low_mark: 0.75,
            freeze_window_secs: 30.0,
            burst_bytes: 0,
        }
    }

    fn stream_enabled_config() -> DrainShapingConfig {
        DrainShapingConfig {
            enabled: true,
            factor: 1.0,
            min_drain_bytes_per_sec: 1,
            max_requests_per_sec: None,
            ewma_alpha: 1.0,
            sample_interval_secs: 1.0,
            engage_min_bytes: 1,
            saturation_high_mark: 0.9,
            saturation_low_mark: 0.75,
            freeze_window_secs: 10.0,
            burst_bytes: 0,
        }
    }

    fn shaped_source(source: &Arc<TestBufferUsageSource>) -> Arc<dyn BufferUsageSource> {
        let cloned_source: Arc<TestBufferUsageSource> = Arc::clone(source);
        cloned_source
    }

    fn assert_invalid(config: DrainShapingConfig, expected: &str) {
        let error = config.validate(true).unwrap_err().to_string();
        assert!(
            error.contains(expected),
            "expected {error:?} to contain {expected:?}"
        );
    }

    #[test]
    fn buffer_usage_observer_implements_buffer_usage_source() {
        fn assert_source<T: BufferUsageSource>() {}

        assert_source::<BufferUsageObserver>();
    }

    #[test]
    fn disabled_config_is_valid_noop() {
        assert!(DrainShapingConfig::default().validate(false).is_ok());
    }

    #[test]
    fn enabled_config_accepts_valid_static_partition_key() {
        valid_enabled_config().validate(true).unwrap();
    }

    #[test]
    fn enabled_config_rejects_dynamic_partition_key() {
        let error = valid_enabled_config()
            .validate(false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("static partition key"));
    }

    #[test]
    fn enabled_config_rejects_invalid_factor() {
        let mut config = valid_enabled_config();
        config.factor = 0.99;
        assert_invalid(config, "factor");

        let mut config = valid_enabled_config();
        config.factor = f64::INFINITY;
        assert_invalid(config, "factor");

        let mut config = valid_enabled_config();
        config.factor = f64::NAN;
        assert_invalid(config, "factor");
    }

    #[test]
    fn enabled_config_rejects_invalid_floor() {
        let mut config = valid_enabled_config();
        config.min_drain_bytes_per_sec = 0;
        assert_invalid(config, "min_drain_bytes_per_sec");
    }

    #[test]
    fn enabled_config_rejects_invalid_ewma_alpha() {
        let mut config = valid_enabled_config();
        config.ewma_alpha = 0.0;
        assert_invalid(config, "ewma_alpha");

        let mut config = valid_enabled_config();
        config.ewma_alpha = 1.01;
        assert_invalid(config, "ewma_alpha");

        let mut config = valid_enabled_config();
        config.ewma_alpha = f64::NAN;
        assert_invalid(config, "ewma_alpha");
    }

    #[test]
    fn enabled_config_rejects_invalid_sample_interval() {
        let mut config = valid_enabled_config();
        config.sample_interval_secs = 0.0;
        assert_invalid(config, "sample_interval_secs");

        let mut config = valid_enabled_config();
        config.sample_interval_secs = f64::INFINITY;
        assert_invalid(config, "sample_interval_secs");
    }

    #[test]
    fn enabled_config_rejects_invalid_freeze_window() {
        let mut config = valid_enabled_config();
        config.freeze_window_secs = 0.0;
        assert_invalid(config, "freeze_window_secs");

        let mut config = valid_enabled_config();
        config.freeze_window_secs = f64::INFINITY;
        assert_invalid(config, "freeze_window_secs");
    }

    #[test]
    fn enabled_config_rejects_invalid_saturation_marks() {
        let mut config = valid_enabled_config();
        config.saturation_low_mark = -0.1;
        assert_invalid(config, "saturation");

        let mut config = valid_enabled_config();
        config.saturation_low_mark = 0.9;
        config.saturation_high_mark = 0.9;
        assert_invalid(config, "saturation");

        let mut config = valid_enabled_config();
        config.saturation_high_mark = 1.01;
        assert_invalid(config, "saturation");
    }

    #[test]
    fn enabled_config_rejects_invalid_request_cap() {
        let mut config = valid_enabled_config();
        config.max_requests_per_sec = Some(0.0);
        assert_invalid(config, "max_requests_per_sec");

        let mut config = valid_enabled_config();
        config.max_requests_per_sec = Some(f64::NAN);
        assert_invalid(config, "max_requests_per_sec");
    }

    #[tokio::test(start_paused = true)]
    async fn disabled_passthrough_no_delay() {
        let source = TestBufferUsageSource::new(1_000);
        source.set_occupancy_bytes(900);
        let config = DrainShapingConfig {
            enabled: false,
            ..stream_enabled_config()
        };
        let start = Instant::now();
        let mut shaped = Box::pin(stream::iter([10, 20, 30]).drain_shaping(
            Some(shaped_source(&source)),
            config,
            |item| *item as usize,
        ));

        assert_eq!(shaped.next().await, Some(10));
        assert_eq!(shaped.next().await, Some(20));
        assert_eq!(shaped.next().await, Some(30));
        assert_eq!(shaped.next().await, None);
        assert_eq!(Instant::now(), start);
    }

    #[tokio::test(start_paused = true)]
    async fn eof_not_delayed_by_debt() {
        let source = TestBufferUsageSource::new(10_000);
        source.set_occupancy_bytes(5_000);
        let mut shaped = Box::pin(stream::iter([1_000]).drain_shaping(
            Some(shaped_source(&source)),
            stream_enabled_config(),
            |item| *item as usize,
        ));
        let start = Instant::now();

        assert_eq!(shaped.next().await, Some(1_000));
        assert_eq!(shaped.next().await, None);
        assert_eq!(Instant::now(), start);
    }

    #[tokio::test(start_paused = true)]
    async fn caps_rate_to_input_times_factor_under_backlog() {
        let source = TestBufferUsageSource::new(10_000);
        source.set_occupancy_bytes(5_000);
        let config = DrainShapingConfig {
            factor: 2.0,
            ..stream_enabled_config()
        };
        let mut shaped = Box::pin(stream::iter([100, 100, 100]).drain_shaping(
            Some(shaped_source(&source)),
            config,
            |item| *item as usize,
        ));
        let start = Instant::now();

        assert_eq!(shaped.next().await, Some(100));
        source.set_received_bytes(100);
        time::advance(Duration::from_secs(1)).await;
        assert_eq!(shaped.next().await, Some(100));

        let mut third = shaped.next();
        assert_eq!(futures::poll!(&mut third), Poll::Pending);
        time::advance(Duration::from_millis(499)).await;
        assert_eq!(futures::poll!(&mut third), Poll::Pending);
        time::advance(Duration::from_millis(1)).await;
        assert_eq!(futures::poll!(&mut third), Poll::Ready(Some(100)));
        assert_eq!(Instant::now() - start, Duration::from_millis(1500));
    }

    #[tokio::test(start_paused = true)]
    async fn engage_passthrough_below_threshold() {
        let source = TestBufferUsageSource::new(10_000);
        source.set_occupancy_bytes(999);
        let config = DrainShapingConfig {
            engage_min_bytes: 1_000,
            ..stream_enabled_config()
        };
        let start = Instant::now();
        let mut shaped = Box::pin(stream::iter([500, 500]).drain_shaping(
            Some(shaped_source(&source)),
            config,
            |item| *item as usize,
        ));

        assert_eq!(shaped.next().await, Some(500));
        assert_eq!(shaped.next().await, Some(500));
        assert_eq!(shaped.next().await, None);
        assert_eq!(Instant::now(), start);
    }

    #[tokio::test(start_paused = true)]
    async fn freeze_prevents_runaway_when_saturated() {
        let source = TestBufferUsageSource::new(1_000);
        source.set_occupancy_bytes(800);
        let mut shaped = Box::pin(
            stream::iter([100, 100, 100, 100, 100, 100, 100, 100]).drain_shaping(
                Some(shaped_source(&source)),
                stream_enabled_config(),
                |item| *item as usize,
            ),
        );

        assert_eq!(shaped.next().await, Some(100));
        source.set_received_bytes(100);
        time::advance(Duration::from_secs(1)).await;
        assert_eq!(shaped.next().await, Some(100));

        source.set_occupancy_bytes(950);
        source.set_received_bytes(10_100);
        time::advance(Duration::from_secs(1)).await;
        assert_eq!(shaped.next().await, Some(100));

        let mut fourth = shaped.next();
        assert_eq!(futures::poll!(&mut fourth), Poll::Pending);
        time::advance(Duration::from_millis(999)).await;
        assert_eq!(futures::poll!(&mut fourth), Poll::Pending);
        time::advance(Duration::from_millis(1)).await;
        assert_eq!(futures::poll!(&mut fourth), Poll::Ready(Some(100)));

        source.set_occupancy_bytes(800);
        source.set_received_bytes(20_100);
        time::advance(Duration::from_secs(1)).await;
        assert_eq!(shaped.next().await, Some(100));

        let mut sixth = shaped.next();
        assert_eq!(futures::poll!(&mut sixth), Poll::Pending);
        time::advance(Duration::from_millis(999)).await;
        assert_eq!(futures::poll!(&mut sixth), Poll::Pending);
        time::advance(Duration::from_millis(1)).await;
        assert_eq!(futures::poll!(&mut sixth), Poll::Ready(Some(100)));

        source.set_occupancy_bytes(700);
        source.set_received_bytes(30_100);
        time::advance(Duration::from_secs(1)).await;
        assert_eq!(shaped.next().await, Some(100));

        let mut eighth = shaped.next();
        assert_eq!(futures::poll!(&mut eighth), Poll::Pending);
        time::advance(Duration::from_millis(9)).await;
        assert_eq!(futures::poll!(&mut eighth), Poll::Pending);
        time::advance(Duration::from_millis(1)).await;
        assert_eq!(futures::poll!(&mut eighth), Poll::Ready(Some(100)));
    }
}
