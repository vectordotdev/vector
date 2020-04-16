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
