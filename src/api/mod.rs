mod handler;
mod schema;
mod server;
pub mod tap;

pub use schema::build_schema;
pub use server::Server;
pub use tap::TapControl;

use tokio::sync::mpsc;

// Control tx/rx channels use Tokio unbounded channels to take advantage of async on the
// receiver end, and sync on the sender. This allows us to trigger a control message when
// the returned stream is canceled externally (e.g. the subscription terminating on the client)
// by implementing `Drop`.

pub type ControlSender = mpsc::UnboundedSender<ControlMessage>;
pub type ControlReceiver = mpsc::UnboundedReceiver<ControlMessage>;

/// Control messages that can be sent from GraphQL requests that affect the operation
/// of topology.
pub enum ControlMessage {
    Tap(tap::TapControl),
}

/// Make an API control channel. The `Sender` is typically sent to `Server::start()` to allow
/// active GraphQL requests to control the operation of topology; `Receiver` responds to user
/// initiated requests.
pub fn make_control() -> (ControlSender, ControlReceiver) {
    mpsc::unbounded_channel()
}
