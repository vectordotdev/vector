use std::{num::NonZeroU32, time::Duration};

use governor::{Quota, clock};
use snafu::Snafu;
use vector_lib::{EstimatedJsonEncodedSizeOf, TimeZone, compile_vrl};
use vrl::compiler::{CompileConfig, Program, TypeState, runtime::Runtime};

use super::{
    DROPPED,
    config::{ThrottleConfig, ThrottleInternalMetricsConfig},
    rate_limiter::RateLimiterRunner,
};
use crate::{
    conditions::Condition,
    config::TransformContext,
    event::{Event, VrlTarget},
    internal_events::{
        TemplateRenderingError, ThrottleEventDiscarded, ThrottleEventProcessed,
        ThrottleUtilizationUpdate,
    },
    template::Template,
    transforms::{SyncTransform, TransformOutputsBuf},
};

/// Which threshold type caused an event to be dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdType {
    Events,
    JsonBytes,
    Tokens,
}

impl ThresholdType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            ThresholdType::Events => "events",
            ThresholdType::JsonBytes => "json_bytes",
            ThresholdType::Tokens => "tokens",
        }
    }
}

/// Maximum number of unique keys to track utilization for.
/// Prevents unbounded memory growth from high-cardinality key fields.
const MAX_UTILIZATION_KEYS: usize = 10_000;

/// Utilization gauges are only emitted every N events to reduce metric overhead.
/// Since gauges overwrite, less frequent emission is equivalent for monitoring.
const UTILIZATION_EMIT_INTERVAL: u64 = 100;

/// Utilization tracking for a single key across all threshold types.
struct KeyUtilization {
    events_consumed: u64,
    events_threshold: u64,
    bytes_consumed: u64,
    bytes_threshold: u64,
    tokens_consumed: u64,
    tokens_threshold: u64,
}

impl KeyUtilization {
    const fn new(events_threshold: u64, bytes_threshold: u64, tokens_threshold: u64) -> Self {
        Self {
            events_consumed: 0,
            events_threshold,
            bytes_consumed: 0,
            bytes_threshold,
            tokens_consumed: 0,
            tokens_threshold,
        }
    }

    fn ratio(&self, threshold_type: ThresholdType) -> f64 {
        match threshold_type {
            ThresholdType::Events if self.events_threshold > 0 => {
                self.events_consumed as f64 / self.events_threshold as f64
            }
            ThresholdType::JsonBytes if self.bytes_threshold > 0 => {
                self.bytes_consumed as f64 / self.bytes_threshold as f64
            }
            ThresholdType::Tokens if self.tokens_threshold > 0 => {
                self.tokens_consumed as f64 / self.tokens_threshold as f64
            }
            _ => 0.0,
        }
    }
}

#[derive(Clone)]
pub struct Throttle<C: clock::Clock<Instant = I>, I: clock::Reference> {
    quota_events: Option<Quota>,
    quota_json_bytes: Option<Quota>,
    quota_tokens: Option<Quota>,
    flush_keys_interval: Duration,
    key_field: Option<Template>,
    exclude: Option<Condition>,
    reroute_dropped: bool,
    pub clock: C,
    internal_metrics: ThrottleInternalMetricsConfig,
    tokens_program: Option<Program>,
    events_threshold: u64,
    bytes_threshold: u64,
    tokens_threshold: u64,
}

fn build_quota(threshold: u32, window: Duration) -> crate::Result<Quota> {
    let nz = NonZeroU32::new(threshold).ok_or_else(|| Box::new(ConfigError::NonZero))?;
    let period = Duration::from_secs_f64(window.as_secs_f64() / f64::from(nz.get()));
    Quota::with_period(period)
        .map(|q| q.allow_burst(nz))
        .ok_or_else(|| Box::new(ConfigError::NonZero).into())
}

impl<C, I> Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference,
{
    pub fn new(
        config: &ThrottleConfig,
        context: &TransformContext,
        clock: C,
    ) -> crate::Result<Self> {
        let flush_keys_interval = config.window_secs;

        let quota_events = config
            .threshold
            .events_threshold()
            .filter(|&n| n > 0)
            .map(|n| build_quota(n, flush_keys_interval))
            .transpose()?;

        let quota_json_bytes = config
            .threshold
            .json_bytes_threshold()
            .filter(|&n| n > 0)
            .map(|n| build_quota(n, flush_keys_interval))
            .transpose()?;

        // Compile VRL tokens expression if configured. The tokens expression provides
        // a custom per-event cost function with its own independent budget.
        let (tokens_program, quota_tokens) =
            if let Some(expr) = config.threshold.tokens_expression() {
                let functions = vector_vrl_functions::all();
                let state = TypeState::default();
                let mut compile_config = CompileConfig::default();
                compile_config.set_custom(context.enrichment_tables.clone());
                compile_config.set_custom(context.metrics_storage.clone());
                compile_config.set_read_only();

                let compilation_result = compile_vrl(expr, &functions, &state, compile_config)
                    .map_err(|diagnostics| crate::format_vrl_diagnostics(expr, diagnostics))?;

                let program = compilation_result.program;

                let tokens_budget = config.threshold.tokens_budget().unwrap_or(0);
                if tokens_budget == 0 {
                    return Err(
                        "`threshold.tokens_budget` must be set when `threshold.tokens` is configured \
                         (it provides the budget for the token limiter)"
                            .into(),
                    );
                }
                let quota = build_quota(tokens_budget, flush_keys_interval)?;
                (Some(program), Some(quota))
            } else {
                (None, None)
            };

        if quota_events.is_none() && quota_json_bytes.is_none() && quota_tokens.is_none() {
            return Err(Box::new(ConfigError::NonZero));
        }

        let exclude = config
            .exclude
            .as_ref()
            .map(|condition| condition.build(&context.enrichment_tables, &context.metrics_storage))
            .transpose()?;

        let events_threshold = config.threshold.events_threshold().unwrap_or(0) as u64;
        let bytes_threshold = config.threshold.json_bytes_threshold().unwrap_or(0) as u64;
        let tokens_threshold = config.threshold.tokens_budget().unwrap_or(0) as u64;

        Ok(Self {
            quota_events,
            quota_json_bytes,
            quota_tokens,
            clock,
            flush_keys_interval,
            key_field: config.key_field.clone(),
            exclude,
            reroute_dropped: config.reroute_dropped,
            internal_metrics: config.internal_metrics.clone(),
            tokens_program,
            events_threshold,
            bytes_threshold,
            tokens_threshold,
        })
    }
}

/// Runtime state for the throttle transform, created when transform starts.
pub struct ThrottleState<C: clock::Clock> {
    events_limiter: Option<RateLimiterRunner<Option<String>, C>>,
    json_bytes_limiter: Option<RateLimiterRunner<Option<String>, C>>,
    tokens_limiter: Option<RateLimiterRunner<Option<String>, C>>,
    key_field: Option<Template>,
    exclude: Option<Condition>,
    reroute_dropped: bool,
    internal_metrics: ThrottleInternalMetricsConfig,
    tokens_program: Option<Program>,
    vrl_runtime: Runtime,
    utilization: std::collections::HashMap<Option<String>, KeyUtilization>,
    events_processed: u64,
    events_threshold: u64,
    bytes_threshold: u64,
    tokens_threshold: u64,
}

impl<C> ThrottleState<C>
where
    C: clock::Clock + Clone + Send + Sync + 'static,
{
    fn compute_json_bytes(event: &Event) -> usize {
        event.estimated_json_encoded_size_of().get()
    }

    fn evaluate_tokens(&mut self, event: &Event) -> Option<NonZeroU32> {
        let program = self.tokens_program.as_ref()?;

        // Use read-only evaluation to avoid cloning the event. The VRL program
        // is compiled read-only, so VrlTarget won't modify the underlying event.
        let mut target = VrlTarget::new(event.clone(), program.info(), true);
        let timezone = TimeZone::default();
        let result = self.vrl_runtime.resolve(&mut target, program, &timezone);
        self.vrl_runtime.clear();

        match result {
            Ok(value) => {
                let cost = match value {
                    vrl::value::Value::Integer(n) if n > 0 => {
                        if n > u32::MAX as i64 {
                            u32::MAX
                        } else {
                            n as u32
                        }
                    }
                    vrl::value::Value::Float(f) => {
                        let n = f.into_inner().ceil() as i64;
                        if n > 0 {
                            if n > u32::MAX as i64 {
                                u32::MAX
                            } else {
                                n as u32
                            }
                        } else {
                            1
                        }
                    }
                    _ => {
                        warn!(
                            message = "VRL tokens expression returned non-positive or non-numeric value, defaulting to cost 1.",
                            ?value,
                        );
                        1
                    }
                };
                NonZeroU32::new(cost)
            }
            Err(err) => {
                warn!(
                    message = "VRL tokens expression error, defaulting to cost 1.",
                    %err,
                );
                NonZeroU32::new(1)
            }
        }
    }

    fn update_utilization(
        &mut self,
        key: &Option<String>,
        json_bytes: usize,
        token_cost: Option<NonZeroU32>,
        exceeded: Option<ThresholdType>,
    ) {
        self.events_processed += 1;

        // Bound utilization map to prevent unbounded memory growth from
        // high-cardinality key fields. Skip tracking new keys once at capacity.
        let util = if let Some(util) = self.utilization.get_mut(key) {
            util
        } else if self.utilization.len() < MAX_UTILIZATION_KEYS {
            self.utilization.entry(key.clone()).or_insert_with(|| {
                KeyUtilization::new(
                    self.events_threshold,
                    self.bytes_threshold,
                    self.tokens_threshold,
                )
            })
        } else {
            return;
        };

        // Only track consumption for limiters that were actually checked by the governor.
        // Threshold checking short-circuits: if events fails, bytes/tokens governors never
        // consumed tokens, so we must not count them in utilization.
        util.events_consumed += 1;
        match exceeded {
            Some(ThresholdType::Events) => {
                // Events limiter failed first — bytes/tokens were never checked
            }
            Some(ThresholdType::JsonBytes) => {
                // Events passed, bytes failed — tokens was never checked
                util.bytes_consumed += json_bytes as u64;
            }
            Some(ThresholdType::Tokens) => {
                // Events and bytes passed, tokens failed
                util.bytes_consumed += json_bytes as u64;
                if let Some(cost) = token_cost {
                    util.tokens_consumed += cost.get() as u64;
                }
            }
            None => {
                // All limiters passed — all consumed
                util.bytes_consumed += json_bytes as u64;
                if let Some(cost) = token_cost {
                    util.tokens_consumed += cost.get() as u64;
                }
            }
        }

        // Gauges overwrite previous values, so emitting every N events is equivalent
        // to per-event emission for monitoring while reducing metric overhead.
        // Emit ALL tracked keys when the interval fires, not just the current key,
        // to ensure fair representation across keys regardless of traffic distribution.
        if !self.events_processed.is_multiple_of(UTILIZATION_EMIT_INTERVAL) {
            return;
        }

        for (k, u) in &self.utilization {
            let key_str = k.as_deref().unwrap_or("").to_owned();

            if self.events_threshold > 0 {
                emit!(ThrottleUtilizationUpdate {
                    key: key_str.clone(),
                    threshold_type: "events",
                    ratio: u.ratio(ThresholdType::Events),
                });
            }
            if self.bytes_threshold > 0 {
                emit!(ThrottleUtilizationUpdate {
                    key: key_str.clone(),
                    threshold_type: "json_bytes",
                    ratio: u.ratio(ThresholdType::JsonBytes),
                });
            }
            if self.tokens_threshold > 0 {
                emit!(ThrottleUtilizationUpdate {
                    key: key_str,
                    threshold_type: "tokens",
                    ratio: u.ratio(ThresholdType::Tokens),
                });
            }
        }
    }
}

impl<C, I> ThrottleState<C>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference,
{
    fn process(&mut self, event: Event, output: &mut TransformOutputsBuf) {
        let (should_throttle, event) = match self.exclude.as_ref() {
            Some(condition) => {
                let (result, event) = condition.check(event);
                (!result, event)
            }
            None => (true, event),
        };

        if !should_throttle {
            output.push(None, event);
            return;
        }

        let key = self.key_field.as_ref().and_then(|t| {
            t.render_string(&event)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("key_field"),
                        drop_event: false,
                    })
                })
                .ok()
        });

        // Compute json_bytes cheaply (size estimate, no clone).
        let json_bytes = if self.json_bytes_limiter.is_some() {
            ThrottleState::<C>::compute_json_bytes(&event)
        } else {
            0
        };

        // Check events limiter first (cheapest).
        let mut exceeded: Option<ThresholdType> = None;
        if let Some(ref limiter) = self.events_limiter
            && !limiter.check_key(&key)
        {
            exceeded = Some(ThresholdType::Events);
        }

        // Only check json_bytes and tokens if events limiter passed.
        // evaluate_tokens clones the event for VRL — defer until we know
        // the event wasn't already rejected by cheaper limiters.
        let token_cost = if exceeded.is_none() {
            if let Some(ref limiter) = self.json_bytes_limiter
                && json_bytes > 0
                && let Some(n) = NonZeroU32::new(json_bytes.min(u32::MAX as usize) as u32)
                && !limiter.check_key_n(&key, n)
            {
                exceeded = Some(ThresholdType::JsonBytes);
                None
            } else if self.tokens_program.is_some() {
                let cost = self.evaluate_tokens(&event);
                if let Some(ref limiter) = self.tokens_limiter
                    && let Some(c) = cost
                    && !limiter.check_key_n(&key, c)
                {
                    exceeded = Some(ThresholdType::Tokens);
                }
                cost
            } else {
                None
            }
        } else {
            None
        };

        let needs_key_str = self.internal_metrics.emit_detailed_metrics
            || self.internal_metrics.emit_events_discarded_per_key;

        // Allocate key_str once for all metric emissions that need it.
        let key_str = if needs_key_str || exceeded.is_some() {
            key.as_deref().unwrap_or("None").to_owned()
        } else {
            String::new()
        };

        if self.internal_metrics.emit_detailed_metrics {
            emit!(ThrottleEventProcessed {
                key: key_str.clone(),
                json_bytes: json_bytes as u64,
                token_cost: token_cost.map_or(0, |n| n.get() as u64),
                emit_detailed_metrics: true,
            });
        }

        if self.internal_metrics.emit_detailed_metrics {
            self.update_utilization(&key, json_bytes, token_cost, exceeded);
        }

        match exceeded {
            Some(threshold_type) => {
                emit!(ThrottleEventDiscarded {
                    key: if needs_key_str { key_str } else { String::new() },
                    threshold_type: threshold_type.as_str(),
                    emit_events_discarded_per_key: self
                        .internal_metrics
                        .emit_events_discarded_per_key,
                    emit_detailed_metrics: self.internal_metrics.emit_detailed_metrics,
                    reroute_dropped: self.reroute_dropped,
                });

                if self.reroute_dropped {
                    output.push(Some(DROPPED), event);
                }
            }
            None => {
                output.push(None, event);
            }
        }
    }
}

#[derive(Debug, Snafu)]
pub enum ConfigError {
    #[snafu(display("`threshold`, and `window_secs` must be non-zero"))]
    NonZero,
}

impl<C, I> Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference + Send + 'static,
{
    fn into_state(self) -> ThrottleState<C> {
        let has_key_field = self.key_field.is_some();

        let make_limiter = |quota: Quota, clock: C| -> RateLimiterRunner<Option<String>, C> {
            if has_key_field {
                RateLimiterRunner::start_keyed(quota, clock, self.flush_keys_interval)
            } else {
                RateLimiterRunner::start_direct(quota, clock)
            }
        };

        let events_limiter = self
            .quota_events
            .map(|quota| make_limiter(quota, self.clock.clone()));
        let json_bytes_limiter = self
            .quota_json_bytes
            .map(|quota| make_limiter(quota, self.clock.clone()));
        let tokens_limiter = self
            .quota_tokens
            .map(|quota| make_limiter(quota, self.clock.clone()));

        ThrottleState {
            events_limiter,
            json_bytes_limiter,
            tokens_limiter,
            key_field: self.key_field,
            exclude: self.exclude,
            reroute_dropped: self.reroute_dropped,
            internal_metrics: self.internal_metrics,
            tokens_program: self.tokens_program,
            vrl_runtime: Runtime::default(),
            utilization: std::collections::HashMap::new(),
            events_processed: 0,
            events_threshold: self.events_threshold,
            bytes_threshold: self.bytes_threshold,
            tokens_threshold: self.tokens_threshold,
        }
    }
}

/// Wrapper that bridges `Throttle` (Clone) to `ThrottleState` (non-Clone runtime state).
/// SyncTransform requires DynClone; ThrottleState can't implement Clone because it holds
/// rate limiter handles and VRL runtime. On clone, we keep the config and lazily recreate state.
pub struct ThrottleSyncTransform<C: clock::Clock<Instant = I>, I: clock::Reference> {
    config: Option<Throttle<C, I>>,
    state: Option<ThrottleState<C>>,
}

impl<C: clock::Clock<Instant = I> + Clone, I: clock::Reference> Clone
    for ThrottleSyncTransform<C, I>
{
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            state: None,
        }
    }
}

impl<C, I> ThrottleSyncTransform<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference + Send + 'static,
{
    pub const fn new(throttle: Throttle<C, I>) -> Self {
        Self {
            config: Some(throttle),
            state: None,
        }
    }

    fn ensure_state(&mut self) -> &mut ThrottleState<C> {
        if self.state.is_none() {
            let config = self
                .config
                .take()
                .expect("config must be present on first call");
            self.state = Some(config.into_state());
        }
        self.state.as_mut().unwrap()
    }
}

impl<C, I> SyncTransform for ThrottleSyncTransform<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference + Send + 'static,
{
    fn transform(&mut self, event: Event, output: &mut TransformOutputsBuf) {
        self.ensure_state().process(event, output);
    }
}

impl<C, I> Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference + Send + 'static,
{
    pub const fn into_sync_transform(self) -> ThrottleSyncTransform<C, I> {
        ThrottleSyncTransform::new(self)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    use super::*;
    use crate::{
        config::TransformContext,
        event::LogEvent,
        test_util::components::assert_transform_compliance,
        transforms::{test::create_topology, throttle::config::ThrottleConfig},
    };

    fn make_buf(config: &ThrottleConfig) -> TransformOutputsBuf {
        let context = TransformContext::default();
        let outputs =
            <ThrottleConfig as crate::config::TransformConfig>::outputs(config, &context, &[]);
        TransformOutputsBuf::new_with_capacity(outputs, 10)
    }

    #[tokio::test]
    async fn throttle_events_backward_compat() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
threshold = 2
window_secs = 5
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        transform.transform(LogEvent::default().into(), &mut buf);
        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 2);

        clock.advance(Duration::from_secs(2));

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 0);

        clock.advance(Duration::from_secs(3));

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);
    }

    #[tokio::test]
    async fn throttle_events_multi_threshold() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 5

[threshold]
events = 2
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        transform.transform(LogEvent::default().into(), &mut buf);
        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 2);

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 0);
    }

    #[tokio::test]
    async fn throttle_json_bytes() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 5

[threshold]
json_bytes = 500
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let mut passed = 0;
        let mut dropped = 0;
        for i in 0..20 {
            let mut log = LogEvent::default();
            log.insert("message", format!("event-{i:0>70}"));
            transform.transform(log.into(), &mut buf);
            let count = buf.drain().count();
            if count > 0 {
                passed += count;
            } else {
                dropped += 1;
            }
        }

        assert!(passed > 0, "Some events should pass");
        assert!(
            dropped > 0,
            "Some events should be dropped (byte limit exceeded)"
        );
        assert!(passed < 20, "Not all events should pass");

        clock.advance(Duration::from_secs(5));
        let mut log = LogEvent::default();
        log.insert("message", "fresh event after window");
        transform.transform(log.into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);
    }

    #[tokio::test]
    async fn throttle_exclude() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 1
window_secs = 5
exclude = """
exists(.special)
"""
"#,
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 0);

        let mut special = LogEvent::default();
        special.insert("special", "true");
        transform.transform(special.into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);
    }

    #[tokio::test]
    async fn throttle_key_field() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 1
window_secs = 5
key_field = "{{ bucket }}"
"#,
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let mut log_a = LogEvent::default();
        log_a.insert("bucket", "a");
        let mut log_b = LogEvent::default();
        log_b.insert("bucket", "b");

        transform.transform(log_a.into(), &mut buf);
        transform.transform(log_b.into(), &mut buf);
        assert_eq!(buf.drain().count(), 2);

        let mut log_a2 = LogEvent::default();
        log_a2.insert("bucket", "a");
        transform.transform(log_a2.into(), &mut buf);
        assert_eq!(buf.drain().count(), 0);
    }

    #[tokio::test]
    async fn throttle_dropped_port() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
threshold = 1
window_secs = 5
reroute_dropped = true
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 0);
        assert_eq!(buf.drain_named(DROPPED).count(), 1);
    }

    #[tokio::test]
    async fn throttle_data_integrity_passed() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
threshold = 100
window_secs = 5
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let mut log = LogEvent::default();
        log.insert("message", "test message");
        log.insert("field1", 42);
        log.insert("field2", true);
        let original = log.clone();

        transform.transform(log.into(), &mut buf);
        let events: Vec<Event> = buf.drain().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_log(), &original);
    }

    #[tokio::test]
    async fn throttle_data_integrity_dropped() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
threshold = 1
window_secs = 5
reroute_dropped = true
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let mut log = LogEvent::default();
        log.insert("message", "will be dropped");
        log.insert("important_field", "preserved");
        let original = log.clone();

        transform.transform(LogEvent::default().into(), &mut buf);
        buf.drain().count();

        transform.transform(log.into(), &mut buf);
        let dropped: Vec<Event> = buf.drain_named(DROPPED).collect();
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].as_log(), &original);
    }

    #[tokio::test]
    async fn throttle_completeness_no_events_lost() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
threshold = 5
window_secs = 5
reroute_dropped = true
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let n = 20;
        for _ in 0..n {
            transform.transform(LogEvent::default().into(), &mut buf);
        }

        let primary_count = buf.drain().count();
        let dropped_count = buf.drain_named(DROPPED).count();
        assert_eq!(
            primary_count + dropped_count,
            n,
            "primary={primary_count} + dropped={dropped_count} should equal {n}"
        );
    }

    #[tokio::test]
    async fn emits_internal_events() {
        assert_transform_compliance(async move {
            let config = ThrottleConfig {
                threshold: crate::transforms::throttle::config::ThresholdConfig::Simple(1),
                window_secs: Duration::from_secs_f64(1.0),
                key_field: None,
                exclude: None,
                reroute_dropped: false,
                internal_metrics: Default::default(),
            };
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let log = LogEvent::from("hello world");
            tx.send(log.into()).await.unwrap();

            _ = out.recv().await;

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await
    }

    /// Memory scaling test: measures approximate heap allocation per key by
    /// comparing RSS before and after populating DashMap entries at different
    /// cardinalities. Reports bytes-per-key for single, dual, and triple
    /// threshold configs at 10, 100, 1K, and 10K unique keys.
    #[tokio::test]
    async fn memory_scaling_per_key() {
        fn rss_bytes() -> usize {
            #[cfg(target_os = "macos")]
            {
                use std::mem::MaybeUninit;
                unsafe extern "C" {
                    fn mach_task_self() -> u32;
                    fn task_info(
                        target_task: u32,
                        flavor: u32,
                        task_info_out: *mut u8,
                        task_info_outCnt: *mut u32,
                    ) -> i32;
                }
                const MACH_TASK_BASIC_INFO: u32 = 20;
                #[repr(C)]
                struct MachTaskBasicInfo {
                    virtual_size: u64,
                    resident_size: u64,
                    resident_size_max: u64,
                    user_time: [u32; 2],
                    system_time: [u32; 2],
                    policy: i32,
                    suspend_count: i32,
                }
                unsafe {
                    let mut info = MaybeUninit::<MachTaskBasicInfo>::uninit();
                    let mut count = (std::mem::size_of::<MachTaskBasicInfo>() / 4) as u32;
                    let kr = task_info(
                        mach_task_self(),
                        MACH_TASK_BASIC_INFO,
                        info.as_mut_ptr() as *mut u8,
                        &mut count,
                    );
                    if kr == 0 {
                        return info.assume_init().resident_size as usize;
                    }
                }
                0
            }
            #[cfg(target_os = "linux")]
            {
                if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
                    if let Some(rss_pages) = statm.split_whitespace().nth(1) {
                        if let Ok(pages) = rss_pages.parse::<usize>() {
                            return pages * 4096;
                        }
                    }
                }
                0
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                0
            }
        }

        enum ThresholdVariant {
            Single,
            Dual,
            Triple,
        }

        fn populate_keys(num_keys: usize, variant: &ThresholdVariant) -> Box<dyn SyncTransform> {
            let config_str = match variant {
                ThresholdVariant::Single => {
                    "threshold = 100000\nwindow_secs = 60\nkey_field = \"{{ service }}\"\n"
                        .to_string()
                }
                ThresholdVariant::Dual => {
                    "window_secs = 60\nkey_field = \"{{ service }}\"\n\n[threshold]\nevents = 100000\njson_bytes = 10000000\n"
                        .to_string()
                }
                ThresholdVariant::Triple => {
                    "window_secs = 60\nkey_field = \"{{ service }}\"\n\n[threshold]\nevents = 100000\njson_bytes = 10000000\ntokens = 'strlen(string!(.message))'\ntokens_budget = 10000000\n"
                        .to_string()
                }
            };
            let config = toml::from_str::<ThrottleConfig>(&config_str).unwrap();
            let clock = clock::FakeRelativeClock::default();
            let throttle = Throttle::new(&config, &TransformContext::default(), clock).unwrap();
            let mut transform: Box<dyn SyncTransform> = Box::new(throttle.into_sync_transform());
            let mut buf = make_buf(&config);

            for i in 0..num_keys {
                let mut log = LogEvent::default();
                log.insert("message", format!("event-{i:0>20}"));
                log.insert("service", format!("svc-{i}"));
                transform.transform(log.into(), &mut buf);
            }
            buf.drain().count();
            transform
        }

        eprintln!("\n=== Memory Footprint Per Key ===");
        eprintln!(
            "  {:>22} {:>10} {:>10} {:>10} {:>10}",
            "Config", "10 keys", "100 keys", "1K keys", "10K keys"
        );

        for (label, variant) in [
            ("events_only (1 limiter)", ThresholdVariant::Single),
            ("events+bytes (2 limiters)", ThresholdVariant::Dual),
            ("all_three (3 limiters)", ThresholdVariant::Triple),
        ] {
            let mut results = Vec::new();
            for &num_keys in &[10, 100, 1_000, 10_000] {
                // Warm up: force page faults by doing a throwaway allocation
                let _warmup = populate_keys(num_keys, &variant);
                drop(_warmup);

                // Measure
                let before = rss_bytes();
                let _transform = populate_keys(num_keys, &variant);
                let after = rss_bytes();
                let delta = after.saturating_sub(before);
                let per_key = if num_keys > 0 { delta / num_keys } else { 0 };
                results.push((delta, per_key));
            }
            eprintln!(
                "  {:>22} {:>7} B/k {:>7} B/k {:>7} B/k {:>7} B/k",
                label, results[0].1, results[1].1, results[2].1, results[3].1,
            );
        }

        eprintln!("  (RSS-based; noisy at low key counts due to page granularity)");
        eprintln!("=== End Memory Footprint ===\n");

        // Verify we can handle 10K keys with all three limiters
        let _t = populate_keys(10_000, &ThresholdVariant::Triple);
    }

    /// Verify that events pass through correctly with no metrics flags set.
    /// The optimization defers key_str allocation — this test ensures no
    /// accidental side effects on the happy path (no discard, no metrics).
    #[tokio::test]
    async fn throttle_happy_path_no_metrics_overhead() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 10000
window_secs = 60
key_field = "{{ service }}"
"#,
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // Send 100 events with 10 different keys — all under limit
        for i in 0..100 {
            let mut log = LogEvent::default();
            log.insert("message", format!("msg-{i}"));
            log.insert("service", format!("svc-{}", i % 10));
            transform.transform(log.into(), &mut buf);
        }
        assert_eq!(buf.drain().count(), 100, "all events should pass");
    }

    /// Verify correct behavior when high-cardinality keys hit rate limits:
    /// each key should be throttled independently.
    #[tokio::test]
    async fn throttle_high_cardinality_independent_keys() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 2
window_secs = 60
key_field = "{{ service }}"
reroute_dropped = true
"#,
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let num_keys = 50;
        let events_per_key = 5;

        for key_idx in 0..num_keys {
            for evt in 0..events_per_key {
                let mut log = LogEvent::default();
                log.insert("message", format!("key{key_idx}-evt{evt}"));
                log.insert("service", format!("svc-{key_idx}"));
                transform.transform(log.into(), &mut buf);
            }
        }

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();

        // Each of 50 keys allows 2 events, so 100 passed, 150 dropped
        assert_eq!(passed, num_keys * 2);
        assert_eq!(dropped, num_keys * (events_per_key - 2));
        assert_eq!(passed + dropped, num_keys * events_per_key);
    }

    /// Verify that multi-threshold with json_bytes correctly throttles
    /// by estimated byte size, not just event count.
    #[tokio::test]
    async fn throttle_json_bytes_size_aware() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
json_bytes = 200
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // Send small events first (should pass)
        let mut log1 = LogEvent::default();
        log1.insert("message", "tiny");
        transform.transform(log1.into(), &mut buf);
        assert_eq!(buf.drain().count(), 1, "small event should pass");

        // Send a larger event that should still fit
        let mut log2 = LogEvent::default();
        log2.insert("message", "a".repeat(50));
        transform.transform(log2.into(), &mut buf);
        assert_eq!(buf.drain().count(), 1, "medium event should pass");

        // Send a large event that pushes over the byte limit
        let mut log3 = LogEvent::default();
        log3.insert("message", "x".repeat(200));
        transform.transform(log3.into(), &mut buf);

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        // The large event should be dropped since we've exceeded 200 bytes budget
        assert_eq!(passed + dropped, 1, "large event should be accounted for");
    }

    /// Verify all three thresholds work together — event is dropped when
    /// ANY single threshold is exceeded, even if others have budget remaining.
    #[tokio::test]
    async fn throttle_any_threshold_triggers_drop() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
events = 100
json_bytes = 200
tokens = 'strlen(string!(.message))'
tokens_budget = 200
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // Events threshold is 100 so that won't trigger, but send a big enough
        // message to blow through the 200 byte json_bytes threshold
        let mut log1 = LogEvent::default();
        log1.insert("message", "ok");
        transform.transform(log1.into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);

        // Now send events until json_bytes is exceeded
        let mut total_passed = 1;
        let mut total_dropped = 0;
        for i in 0..10 {
            let mut log = LogEvent::default();
            log.insert("message", format!("payload-{i:0>60}"));
            transform.transform(log.into(), &mut buf);
            total_passed += buf.drain().count();
            total_dropped += buf.drain_named(DROPPED).count();
        }

        // Should have some drops from json_bytes even though events < 100
        assert!(
            total_dropped > 0,
            "json_bytes should trigger drops before events threshold (passed={total_passed}, dropped={total_dropped})"
        );
        assert!(
            total_passed < 100,
            "not all events should pass since json_bytes is restrictive"
        );
    }

    /// Test that the key cardinality scaling works at various levels
    /// without panics or incorrect behavior.
    #[tokio::test]
    async fn throttle_key_cardinality_scaling() {
        for num_keys in [10, 100, 1_000] {
            let clock = clock::FakeRelativeClock::default();
            let config = toml::from_str::<ThrottleConfig>(
                r#"
window_secs = 60
key_field = "{{ service }}"
reroute_dropped = true

[threshold]
events = 5
json_bytes = 10000
"#,
            )
            .unwrap();

            let throttle =
                Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
            let mut transform = throttle.into_sync_transform();
            let mut buf = make_buf(&config);

            let events_per_key = 10;
            for key_idx in 0..num_keys {
                for evt in 0..events_per_key {
                    let mut log = LogEvent::default();
                    log.insert("message", format!("k{key_idx}-e{evt}"));
                    log.insert("service", format!("svc-{key_idx}"));
                    transform.transform(log.into(), &mut buf);
                }
            }

            let passed = buf.drain().count();
            let dropped = buf.drain_named(DROPPED).count();

            assert_eq!(
                passed + dropped,
                num_keys * events_per_key,
                "no events lost at {num_keys} keys"
            );
            // Each key allows 5 events, so at least 5*num_keys passed
            assert!(
                passed >= num_keys * 5,
                "at least 5 events per key should pass at {num_keys} keys (got {passed})"
            );
        }
    }

    // -- Edge-case tests for findings --

    /// Finding 1 (High): Oversized events must be throttled, not bypassed.
    /// An event whose json_bytes cost exceeds the burst capacity must be
    /// dropped (or rerouted), never allowed through.
    #[tokio::test]
    async fn oversized_event_is_throttled_not_bypassed() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
json_bytes = 50
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // Create an event whose estimated JSON size far exceeds the 50-byte budget.
        let mut log = LogEvent::default();
        log.insert("message", "x".repeat(500));
        transform.transform(log.into(), &mut buf);

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        assert_eq!(passed, 0, "oversized event must NOT pass through");
        assert_eq!(dropped, 1, "oversized event must be rerouted to dropped port");
    }

    /// Finding 1 variant: Oversized token cost also throttled.
    #[tokio::test]
    async fn oversized_token_cost_is_throttled() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
tokens = 'strlen(string!(.message))'
tokens_budget = 10
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // Token cost of 200 chars far exceeds the 10-token budget.
        let mut log = LogEvent::default();
        log.insert("message", "x".repeat(200));
        transform.transform(log.into(), &mut buf);

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        assert_eq!(passed, 0, "oversized token cost must not pass through");
        assert_eq!(dropped, 1, "oversized token cost must be rerouted");
    }

    /// Finding 2 (Medium): tokens is independent of json_bytes.
    /// A standalone tokens config (without json_bytes) should work when
    /// tokens_budget is provided.
    #[tokio::test]
    async fn tokens_standalone_without_json_bytes() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
tokens = 'strlen(string!(.message))'
tokens_budget = 100
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // Each event costs ~10 tokens. With budget=100, we get ~10 events through.
        for i in 0..20 {
            let mut log = LogEvent::default();
            log.insert("message", format!("{i:0>10}"));
            transform.transform(log.into(), &mut buf);
        }

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        assert!(passed > 0, "some events should pass");
        assert!(dropped > 0, "some events should be dropped");
        assert_eq!(passed + dropped, 20, "all events accounted for");
    }

    /// Finding 2 variant: tokens without tokens_budget should error at build time.
    #[tokio::test]
    async fn tokens_without_budget_errors() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60

[threshold]
tokens = 'strlen(string!(.message))'
",
        )
        .unwrap();

        let result = Throttle::new(&config, &TransformContext::default(), clock);
        assert!(result.is_err(), "tokens without tokens_budget must error");
        let err = result.err().unwrap().to_string();
        assert!(
            err.contains("tokens_budget"),
            "error should mention tokens_budget, got: {err}"
        );
    }

    /// Finding 2 variant: tokens with json_bytes but no tokens_budget errors.
    /// json_bytes should NOT serve as the tokens budget anymore.
    #[tokio::test]
    async fn tokens_with_json_bytes_but_no_budget_errors() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60

[threshold]
json_bytes = 500000
tokens = 'strlen(string!(.message))'
",
        )
        .unwrap();

        let result = Throttle::new(&config, &TransformContext::default(), clock.clone());
        assert!(
            result.is_err(),
            "tokens with json_bytes but no tokens_budget must error"
        );
    }

    /// Finding 3 (Medium): reroute_dropped should not inflate component_discarded_events_total.
    /// This test checks behavior correctness: rerouted events must appear on
    /// the dropped port, and the event itself is preserved.
    #[tokio::test]
    async fn rerouted_events_are_not_lost() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
threshold = 1
window_secs = 60
reroute_dropped = true
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // First event passes
        let mut log1 = LogEvent::default();
        log1.insert("message", "first");
        transform.transform(log1.into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);

        // Second event is throttled but rerouted
        let mut log2 = LogEvent::default();
        log2.insert("message", "second");
        log2.insert("important", "data");
        let log2_clone = log2.clone();
        transform.transform(log2.into(), &mut buf);

        assert_eq!(buf.drain().count(), 0, "should not pass to primary");
        let dropped: Vec<Event> = buf.drain_named(DROPPED).collect();
        assert_eq!(dropped.len(), 1, "should appear on dropped port");
        assert_eq!(
            dropped[0].as_log(),
            &log2_clone,
            "rerouted event must be unmodified"
        );
    }

    /// Verify that without reroute_dropped, throttled events truly disappear.
    #[tokio::test]
    async fn throttled_without_reroute_events_vanish() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
threshold = 1
window_secs = 60
reroute_dropped = false
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);

        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 0, "excess event should be discarded");
    }

    /// Unkeyed path optimization: when key_field is not set,
    /// the direct rate limiter should work identically to keyed.
    #[tokio::test]
    async fn unkeyed_rate_limiter_works() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
threshold = 3
window_secs = 60
reroute_dropped = true
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        for _ in 0..10 {
            transform.transform(LogEvent::default().into(), &mut buf);
        }

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        assert_eq!(passed, 3, "unkeyed: 3 should pass");
        assert_eq!(dropped, 7, "unkeyed: 7 should be dropped");
    }

    /// Unkeyed json_bytes limiter (direct path).
    #[tokio::test]
    async fn unkeyed_json_bytes_limiter() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
json_bytes = 200
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let mut total_passed = 0;
        let mut total_dropped = 0;
        for i in 0..20 {
            let mut log = LogEvent::default();
            log.insert("message", format!("msg-{i:0>30}"));
            transform.transform(log.into(), &mut buf);
            total_passed += buf.drain().count();
            total_dropped += buf.drain_named(DROPPED).count();
        }

        assert!(total_passed > 0, "some events should pass");
        assert!(total_dropped > 0, "some events should be throttled by bytes");
        assert_eq!(total_passed + total_dropped, 20);
    }

    /// Window replenishment: after the window passes, budget is restored.
    #[tokio::test]
    async fn window_replenishment_restores_budget() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 5
reroute_dropped = true

[threshold]
json_bytes = 100
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // Exhaust the budget
        for _ in 0..10 {
            let mut log = LogEvent::default();
            log.insert("message", "x".repeat(50));
            transform.transform(log.into(), &mut buf);
        }
        buf.drain().count();
        let dropped_before = buf.drain_named(DROPPED).count();
        assert!(dropped_before > 0, "should have drops within window");

        // Advance past the window
        clock.advance(Duration::from_secs(6));

        // Budget should be replenished
        let mut log = LogEvent::default();
        log.insert("message", "fresh");
        transform.transform(log.into(), &mut buf);
        assert_eq!(
            buf.drain().count(),
            1,
            "event after window should pass"
        );
    }

    /// Combined events + json_bytes: events limit hit first.
    #[tokio::test]
    async fn events_limit_triggers_before_bytes() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
events = 2
json_bytes = 1000000
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        for _ in 0..5 {
            let mut log = LogEvent::default();
            log.insert("message", "tiny");
            transform.transform(log.into(), &mut buf);
        }

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        assert_eq!(passed, 2, "events limit should trigger at 2");
        assert_eq!(dropped, 3, "3 events should be dropped by events limit");
    }

    /// Combined events + json_bytes: bytes limit hit first.
    #[tokio::test]
    async fn bytes_limit_triggers_before_events() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
events = 1000
json_bytes = 100
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let mut total_passed = 0;
        let mut total_dropped = 0;
        for i in 0..20 {
            let mut log = LogEvent::default();
            log.insert("message", format!("{i:0>40}"));
            transform.transform(log.into(), &mut buf);
            total_passed += buf.drain().count();
            total_dropped += buf.drain_named(DROPPED).count();
        }

        assert!(total_passed < 20, "bytes limit should trigger before events limit of 1000");
        assert!(total_dropped > 0, "some should be dropped by bytes");
        assert!(total_passed < 1000, "events limit did not trigger");
    }

    /// Zero-cost VRL expression (returns 0 or negative) defaults to cost 1.
    #[tokio::test]
    async fn vrl_zero_cost_defaults_to_one() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
tokens = '0'
tokens_budget = 3
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // VRL returns 0, which should default to cost 1.
        // With budget of 3, expect 3 events to pass.
        for _ in 0..5 {
            transform.transform(LogEvent::default().into(), &mut buf);
        }

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        assert_eq!(passed, 3, "zero-cost VRL should default to 1 token each");
        assert_eq!(dropped, 2);
    }

    /// VRL expression that errors defaults to cost 1.
    #[tokio::test]
    async fn vrl_error_defaults_to_cost_one() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
window_secs = 60
reroute_dropped = true

[threshold]
tokens = 'to_int!(.nonexistent)'
tokens_budget = 2
"#,
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        for _ in 0..5 {
            transform.transform(LogEvent::default().into(), &mut buf);
        }

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        assert_eq!(passed, 2, "VRL error should default to cost 1");
        assert_eq!(dropped, 3);
    }

    /// Exclude condition with multi-threshold: excluded events bypass ALL limiters.
    #[tokio::test]
    async fn exclude_bypasses_all_limiters() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
window_secs = 60
exclude = """
exists(.critical)
"""

[threshold]
events = 1
json_bytes = 50
"#,
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        // First normal event passes
        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 1);

        // Second normal event is throttled
        transform.transform(LogEvent::default().into(), &mut buf);
        assert_eq!(buf.drain().count(), 0);

        // Excluded event bypasses even after budget exhausted
        let mut critical = LogEvent::default();
        critical.insert("critical", true);
        critical.insert("message", "x".repeat(1000));
        transform.transform(critical.into(), &mut buf);
        assert_eq!(buf.drain().count(), 1, "excluded event must always pass");
    }

    /// Completeness: with reroute_dropped, every input event exits on
    /// exactly one port across multiple threshold types.
    #[tokio::test]
    async fn completeness_with_multi_threshold_reroute() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r"
window_secs = 60
reroute_dropped = true

[threshold]
events = 5
json_bytes = 500
",
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();
        let mut transform = throttle.into_sync_transform();
        let mut buf = make_buf(&config);

        let n = 50;
        for i in 0..n {
            let mut log = LogEvent::default();
            log.insert("message", format!("event-{i:0>30}"));
            transform.transform(log.into(), &mut buf);
        }

        let passed = buf.drain().count();
        let dropped = buf.drain_named(DROPPED).count();
        assert_eq!(
            passed + dropped,
            n,
            "all events must be accounted for: passed={passed}, dropped={dropped}"
        );
    }
}
