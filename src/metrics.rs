use crate::event::{Event, Metric};
use metrics_runtime::{Controller, Receiver};
use once_cell::sync::OnceCell;

pub static CONTROLLER: OnceCell<Controller> = OnceCell::new();

pub fn init() -> crate::Result<()> {
    let receiver = Receiver::builder()
        .build()
        .expect("failed to create receiver");

    CONTROLLER
        .set(receiver.controller())
        .map_err(|_| "failed to set receiver. metrics system already initialized.")?;

    receiver.install();

    Ok(())
}

pub fn get_controller() -> crate::Result<Controller> {
    CONTROLLER
        .get()
        .cloned()
        .ok_or_else(|| "metrics system not initialized".into())
}

pub fn capture_metrics(controller: &Controller) -> impl Iterator<Item = Event> {
    controller
        .snapshot()
        .into_measurements()
        .into_iter()
        .map(|(k, m)| Metric::from_measurement(k, m).into())
}
