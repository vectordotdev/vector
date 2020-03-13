use metrics_runtime::{Controller, Receiver};
use once_cell::sync::OnceCell;

pub static CONTROLLER: OnceCell<Controller> = OnceCell::new();

pub fn init() {
    let receiver = Receiver::builder()
        .build()
        .expect("failed to create receiver");

    if CONTROLLER.set(receiver.controller()).is_err() {
        panic!("failed to set receiver. metrics system already initialized.")
    };

    receiver.install();
}
