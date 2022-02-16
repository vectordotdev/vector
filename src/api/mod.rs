mod handler;
mod schema;
mod server;
pub(super) mod tap;

use schema::build_schema;
pub use server::Server;
use tokio::sync::oneshot;

// Shutdown channel types used by the server and tap.
type ShutdownTx = oneshot::Sender<()>;
type ShutdownRx = oneshot::Receiver<()>;
