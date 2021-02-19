use tokio::sync::mpsc;

mod handler;
mod schema;
mod server;

pub use schema::build_schema;
pub use server::Server;

pub type Sender = mpsc::Sender<String>;
pub type Receiver = mpsc::Receiver<String>;

/// Make an API controller
pub fn make_control() -> (Sender, Receiver) {
    mpsc::channel(100)
}
