use crate::{event::Metric, Event};
use metrics::{Key, Recorder};
use metrics_util::{CompositeKey, Handle, MetricKind, Registry};
use once_cell::sync::OnceCell;

static CONTROLLER: OnceCell<Controller> = OnceCell::new();

pub fn init() -> crate::Result<()> {
    CONTROLLER
        .set(Controller::new())
        .map_err(|_| "controller already initialized")?;

    metrics::set_recorder(CONTROLLER.get().unwrap()).map_err(|_| "recorder already initialized")?;

    Ok(())
}

pub struct Controller {
    registry: Registry<CompositeKey, Handle>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            registry: Registry::new(),
        }
    }
}

impl Recorder for Controller {
    fn register_counter(&self, key: Key, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Counter, key);
        self.registry.op(ckey, |_| {}, || Handle::counter())
    }
    fn register_gauge(&self, key: Key, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Gauge, key);
        self.registry.op(ckey, |_| {}, || Handle::gauge())
    }
    fn register_histogram(&self, key: Key, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Histogram, key);
        self.registry.op(ckey, |_| {}, || Handle::histogram())
    }

    fn increment_counter(&self, key: Key, value: u64) {
        let ckey = CompositeKey::new(MetricKind::Counter, key);
        self.registry.op(
            ckey,
            |handle| handle.increment_counter(value),
            || Handle::counter(),
        )
    }
    fn update_gauge(&self, key: Key, value: f64) {
        let ckey = CompositeKey::new(MetricKind::Gauge, key);
        self.registry.op(
            ckey,
            |handle| handle.update_gauge(value),
            || Handle::gauge(),
        )
    }
    fn record_histogram(&self, key: Key, value: u64) {
        let ckey = CompositeKey::new(MetricKind::Histogram, key);
        self.registry.op(
            ckey,
            |handle| handle.record_histogram(value),
            || Handle::histogram(),
        )
    }
}

pub fn get_controller() -> crate::Result<&'static Controller> {
    CONTROLLER
        .get()
        .ok_or_else(|| "metrics system not initialized".into())
}

pub fn snapshot(controller: &Controller) -> Vec<Event> {
    let handles = controller.registry.get_handles();
    handles
        .into_iter()
        .map(|(ck, m)| {
            let (_, k) = ck.into_parts();
            Metric::from_metric_kv(k, m).into()
        })
        .collect()
}

pub fn capture_metrics(controller: &Controller) -> impl Iterator<Item = Event> {
    snapshot(controller).into_iter()
}
