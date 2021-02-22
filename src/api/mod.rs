mod handler;
mod schema;
mod server;
pub mod tap;

pub use schema::build_schema;
pub use server::Server;
pub use tap::TapControl;

use tokio::sync::mpsc;

pub type ControlSender = mpsc::Sender<ControlMessage>;
pub type ControlReceiver = mpsc::Receiver<ControlMessage>;

/// Control messages that can be sent from GraphQL requests that affect the operation
/// of topology.
pub enum ControlMessage {
    Tap(tap::TapControl),
}

/// Make an API control channel. The `Sender` is typically sent to `Server::start()` to allow
/// active GraphQL requests to control the operation of topology; `Receiver` responds to user
/// initiated requests.
pub fn make_control<'a>() -> (ControlSender, ControlReceiver) {
    mpsc::channel(100)
}
