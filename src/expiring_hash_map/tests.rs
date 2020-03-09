use super::*;
use crate::runtime::Runtime;
use futures::future::poll_fn;
use futures_test::task::{noop_context, panic_context};
use std::time::{Duration, Instant};

#[test]
fn poll_does_not_return_ready_with_empty_map() {
    let mut map = ExpiringHashMap::<String, String>::new();
    let mut cx = noop_context();
    assert!(map.poll_expired(&mut cx).is_pending());
}

#[test]
fn poll_does_not_return_ready_with_empty_map_after_it_was_non_empty() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on_std(async {
        let mut map = ExpiringHashMap::<String, String>::new();

        map.insert("key".to_owned(), "val".to_owned(), Duration::from_secs(1));
        map.remove("key");

        let mut cx = noop_context();
        assert!(map.poll_expired(&mut cx).is_pending());
    });
}

#[test]
fn it_does_not_call_waker_if_polled_and_ready() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on_std(async {
        let mut map = ExpiringHashMap::<String, String>::new();

        let a_minute_ago = Instant::now() - Duration::from_secs(60);
        map.insert_at("key".to_owned(), "val".to_owned(), a_minute_ago);

        let mut cx = panic_context();
        assert!(map.poll_expired(&mut cx).is_ready());
    });
}

#[test]
fn it_returns_expired_values() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on_std(async {
        let mut map = ExpiringHashMap::<String, String>::new();

        let a_minute_ago = Instant::now() + Duration::from_secs(1);
        map.insert_at("key".to_owned(), "val".to_owned(), a_minute_ago);

        let fut = poll_fn(move |cx| map.poll_expired(cx));
        assert_eq!(fut.await.unwrap().0, "val".to_owned());
    });
}
