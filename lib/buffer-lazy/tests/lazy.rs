use buffer_lazy::BufferLazy;
use futures::prelude::*;
use std::thread;
use tokio_executor::{SpawnError, TypedExecutor};
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn lazy() {
    let (service, mut handle) = mock::pair::<(), ()>();

    let mut buf1 = BufferLazy::with_executor(service, 1, Exec);
    let mut buf2 = buf1.clone();
    let mut buf3 = buf2.clone();

    assert!(buf2.poll_ready().unwrap().is_ready());
    let response = buf2.call(());

    assert_request_eq!(handle, ()).send_response(());

    assert_eq!(response.wait().unwrap(), ());

    assert!(buf3.poll_ready().unwrap().is_ready());
    let response = buf3.call(());

    assert_request_eq!(handle, ()).send_response(());

    assert_eq!(response.wait().unwrap(), ());

    assert!(buf1.poll_ready().unwrap().is_ready());
    let response = buf1.call(());

    assert_request_eq!(handle, ()).send_response(());

    assert_eq!(response.wait().unwrap(), ());
}

#[derive(Clone)]
struct Exec;

impl<F> TypedExecutor<F> for Exec
where
    F: Future<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, fut: F) -> Result<(), SpawnError> {
        thread::spawn(move || {
            fut.wait().unwrap();
        });
        Ok(())
    }
}
