mod handler;
mod schema;
mod server;
pub mod tap;

pub use schema::build_schema;
pub use server::Server;

use tokio::sync::oneshot;

type ShutdownTx = oneshot::Sender<()>;
type ShutdownRx = oneshot::Receiver<()>;
