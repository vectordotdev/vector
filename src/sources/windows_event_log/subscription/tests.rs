use super::*;
use serial_test::serial;

async fn create_test_checkpointer() -> (Arc<Checkpointer>, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let checkpointer = Arc::new(Checkpointer::new(temp_dir.path()).await.unwrap());
    (checkpointer, temp_dir)
}

#[test]
fn test_rate_limiter_configuration() {
    let mut config = WindowsEventLogConfig::default();
    assert_eq!(config.events_per_second, 0);

    config.events_per_second = 1000;
    assert_eq!(config.events_per_second, 1000);
}

#[tokio::test]
async fn test_rate_limiter_disabled_by_default() {
    let config = WindowsEventLogConfig::default();
    assert_eq!(
        config.events_per_second, 0,
        "Rate limiting should be disabled by default"
    );
}

/// Test pull subscription creation and basic operation
#[tokio::test]
async fn test_pull_subscription_creation() {
    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.event_timeout_ms = 1000;

    let (checkpointer, _temp_dir) = create_test_checkpointer().await;

    let subscription = EventLogSubscription::new(&config, checkpointer, false).await;
    assert!(
        subscription.is_ok(),
        "Pull subscription creation should succeed: {:?}",
        subscription.err()
    );

    let sub = subscription.unwrap();
    assert_eq!(
        sub.channels.len(),
        1,
        "Should have one channel subscription"
    );
}

/// Test that wait_for_events_blocking returns timeout or events available
#[tokio::test]
async fn test_wait_for_events_timeout() {
    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.read_existing_events = false;
    config.event_timeout_ms = 100;

    let (checkpointer, _temp_dir) = create_test_checkpointer().await;

    let subscription = EventLogSubscription::new(&config, checkpointer, false)
        .await
        .expect("Subscription creation should succeed");

    // Use ownership transfer pattern for spawn_blocking
    let (subscription, result) = tokio::task::spawn_blocking(move || {
        let r = subscription.wait_for_events_blocking(100);
        (subscription, r)
    })
    .await
    .unwrap();

    // The first call may return EventsAvailable since signals are initially signaled.
    // That's expected behavior per the pull model design.
    match result {
        WaitResult::EventsAvailable | WaitResult::Timeout => {}
        WaitResult::Shutdown => panic!("Should not get shutdown"),
    }

    // Keep subscription alive until end of test
    drop(subscription);
}

/// Test that signal_shutdown wakes a waiting thread
#[tokio::test]
async fn test_shutdown_signal_wakes_wait() {
    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.event_timeout_ms = 500;

    let (checkpointer, _temp_dir) = create_test_checkpointer().await;

    let subscription = EventLogSubscription::new(&config, checkpointer, false)
        .await
        .expect("Subscription creation should succeed");

    // First drain the initially-signaled state using ownership transfer
    let (subscription, _) = tokio::task::spawn_blocking(move || {
        let r = subscription.wait_for_events_blocking(50);
        (subscription, r)
    })
    .await
    .unwrap();

    let shutdown_event_raw = subscription.shutdown_event_raw() as isize;

    let wait_handle = tokio::task::spawn_blocking(move || {
        let r = subscription.wait_for_events_blocking(30000);
        (subscription, r)
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    unsafe {
        let handle = HANDLE(shutdown_event_raw as *mut std::ffi::c_void);
        let _ = SetEvent(handle);
    }

    let (subscription, result) = wait_handle.await.unwrap();
    match result {
        WaitResult::Shutdown => {} // Expected
        WaitResult::EventsAvailable => {
            // Acceptable - there may have been real events
        }
        WaitResult::Timeout => {
            panic!("Should not timeout - shutdown should have woken the wait");
        }
    }

    drop(subscription);
}

/// Test that shutdown wins when both shutdown and channel handles are signaled.
#[tokio::test]
async fn test_shutdown_signal_takes_priority_over_channel_signal() {
    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.event_timeout_ms = 500;

    let (checkpointer, _temp_dir) = create_test_checkpointer().await;

    let subscription = EventLogSubscription::new(&config, checkpointer, false)
        .await
        .expect("Subscription creation should succeed");

    unsafe {
        let handle = HANDLE(subscription.shutdown_event_raw());
        let _ = SetEvent(handle);
    }

    let result = subscription.wait_for_events_blocking(0);
    assert!(
        matches!(result, WaitResult::Shutdown),
        "shutdown should take priority over already-signaled channels"
    );
}

/// Test pull_events with read_existing_events=true
#[tokio::test]
async fn test_pull_events_returns_events() {
    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.read_existing_events = true;
    config.event_timeout_ms = 2000;

    let (checkpointer, _temp_dir) = create_test_checkpointer().await;

    let subscription = EventLogSubscription::new(&config, checkpointer, false)
        .await
        .expect("Subscription creation should succeed");

    // Wait and pull using ownership transfer pattern
    let (mut subscription, wait_result) = tokio::task::spawn_blocking(move || {
        let r = subscription.wait_for_events_blocking(2000);
        (subscription, r)
    })
    .await
    .unwrap();

    match wait_result {
        WaitResult::EventsAvailable => {
            let events = subscription.pull_events(100).unwrap();
            assert!(
                !events.is_empty(),
                "With read_existing_events=true, should get historical events"
            );
        }
        WaitResult::Timeout => {
            // Might happen on a system with empty Application log
        }
        WaitResult::Shutdown => panic!("Unexpected shutdown"),
    }
}

/// Test multiple concurrent pull subscriptions
#[tokio::test]
async fn test_multiple_concurrent_subscriptions() {
    let mut config1 = WindowsEventLogConfig::default();
    config1.channels = vec!["Application".to_string()];
    config1.event_timeout_ms = 1000;

    let mut config2 = WindowsEventLogConfig::default();
    config2.channels = vec!["System".to_string()];
    config2.event_timeout_ms = 1000;

    let (checkpointer1, _temp_dir1) = create_test_checkpointer().await;
    let (checkpointer2, _temp_dir2) = create_test_checkpointer().await;

    let sub1 = EventLogSubscription::new(&config1, checkpointer1, false)
        .await
        .expect("Subscription 1 (Application) should succeed");
    let sub2 = EventLogSubscription::new(&config2, checkpointer2, false)
        .await
        .expect("Subscription 2 (System) should succeed");

    // Both should be independently functional
    assert_eq!(sub1.channels.len(), 1);
    assert_eq!(sub2.channels.len(), 1);
    assert_eq!(sub1.channels[0].channel, "Application");
    assert_eq!(sub2.channels[0].channel, "System");
}

/// Test read_existing_events=false only receives future events
#[tokio::test]
async fn test_read_existing_events_false_only_receives_future_events() {
    use chrono::Utc;

    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.read_existing_events = false;
    config.event_timeout_ms = 500;

    let (checkpointer, _temp_dir) = create_test_checkpointer().await;
    let subscription_start_time = Utc::now();

    let mut subscription = EventLogSubscription::new(&config, checkpointer, false)
        .await
        .expect("Subscription creation should succeed");

    // Brief wait then pull
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let events = subscription.pull_events(100).unwrap_or_default();

    let tolerance = chrono::Duration::seconds(5);
    let earliest_allowed = subscription_start_time - tolerance;

    for event in &events {
        assert!(
            event.time_created >= earliest_allowed,
            "Event timestamp {} is before subscription start time {} (minus tolerance). \
             read_existing_events=false may not be respected. Event ID: {}, Record ID: {}",
            event.time_created,
            subscription_start_time,
            event.event_id,
            event.record_id
        );
    }
}

/// Test that subscription gracefully handles an invalid/corrupted bookmark
/// from a checkpoint, falling back to a fresh bookmark without crashing.
#[tokio::test]
async fn test_checkpoint_with_invalid_bookmark_falls_back_gracefully() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let checkpointer = Arc::new(Checkpointer::new(temp_dir.path()).await.unwrap());

    let fake_bookmark = r#"<BookmarkList><Bookmark Channel='Application' RecordId='999999999' IsCurrent='true'/></BookmarkList>"#;

    checkpointer
        .set("Application".to_string(), fake_bookmark.to_string())
        .await
        .expect("Should be able to set checkpoint");

    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.read_existing_events = true;
    config.event_timeout_ms = 500;

    // The subscription should succeed even with a corrupted/invalid bookmark,
    // gracefully falling back to a fresh bookmark.
    let mut subscription = EventLogSubscription::new(&config, checkpointer, false)
        .await
        .expect("Subscription should succeed even with invalid bookmark checkpoint");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Just verify we can pull events without panicking.
    // The bookmark format above is not a real Windows bookmark, so the
    // subscription will fall back to reading from scratch. We only assert
    // that the subscription is functional.
    let _events = subscription.pull_events(100).unwrap_or_default();
}

/// Proves that `pull_events` works independently of signal state — the
/// invariant the speculative timeout pull in mod.rs relies on.
///
/// Steps:
/// 1. Subscribe to the Application log with `read_existing_events = true`.
/// 2. Manually clear the channel signal via `ResetEvent`, simulating a lost wakeup.
/// 3. Assert `wait_for_events_blocking` times out (signal cleared, no OS wake-up).
/// 4. Assert `pull_events` still returns events — `EvtNext` fetches from the queue
///    regardless of signal state, so the speculative pull in mod.rs self-heals.
#[tokio::test]
#[serial]
async fn test_pull_events_works_with_cleared_signal() {
    // Seed the Application log with a record so the "events remain
    // available despite cleared signal" assertion below does not depend
    // on whatever backlog the runner happens to have. Freshly provisioned
    // CI images can have an empty Application log, which would otherwise
    // make `pull_events` legitimately return empty and produce a spurious
    // failure unrelated to the invariant under test.
    let seed_output = std::process::Command::new("eventcreate")
        .args([
            "/T",
            "INFORMATION",
            "/ID",
            "100",
            "/L",
            "APPLICATION",
            "/SO",
            "VectorTestSpeculativePullSeed",
            "/D",
            "seed event for #25194 speculative-pull regression test",
        ])
        .output()
        .expect("failed to spawn eventcreate — required for deterministic seeding");
    assert!(
        seed_output.status.success(),
        "eventcreate failed to seed Application log (exit={:?}): stdout={:?} stderr={:?}. \
         This test requires a seeded event to be deterministic; a locked-down runner \
         without the privilege to write to Application cannot run this test reliably.",
        seed_output.status.code(),
        String::from_utf8_lossy(&seed_output.stdout),
        String::from_utf8_lossy(&seed_output.stderr),
    );
    // Give the service a moment to persist the record before we subscribe.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.read_existing_events = true;
    config.event_timeout_ms = 500;

    let (checkpointer, _temp_dir) = create_test_checkpointer().await;

    let mut subscription = EventLogSubscription::new(&config, checkpointer, false)
        .await
        .expect("Subscription creation should succeed");

    // Manually clear the signal to simulate a lost wakeup. The seeded
    // event above guarantees at least one record is queued in EvtNext
    // regardless of the runner's pre-existing log state.
    let signal_raw = subscription.first_channel_signal_raw();
    unsafe {
        let _ = ResetEvent(HANDLE(signal_raw as *mut std::ffi::c_void));
    }

    // Signal is cleared: an immediate (0ms) poll must report Timeout.
    // A 0ms wait reads only the current signal state with no grace
    // window, so unrelated Windows system events arriving between the
    // `ResetEvent` above and the poll cannot re-signal the handle and
    // cause a spurious failure.
    let wait_result = subscription.wait_for_events_blocking(0);

    assert!(
        matches!(wait_result, WaitResult::Timeout),
        "expected Timeout after manual ResetEvent; signal was not cleared"
    );

    // Despite the cleared signal, pull_events must still return events.
    // This is the invariant the speculative timeout pull in mod.rs depends on.
    let events = subscription.pull_events(100).unwrap_or_default();
    assert!(
        !events.is_empty(),
        "pull_events must return events independently of signal state; \
         this is the invariant the speculative timeout pull in mod.rs depends on"
    );
}

/// Regression test for vectordotdev/vector#25194.
///
/// The Windows Event Log service signals the pull-mode wait handle via
/// `SetEvent` each time a new matching event is recorded. Because the
/// handle is manual-reset, `SetEvent` on an already-signaled handle is
/// a no-op. If `pull_events` resets the signal *after* draining events
/// via `EvtNext`, any signal that fires between the last `EvtNext` and
/// the `ResetEvent` call is silently lost — the subscription then
/// permanently hangs until a subsequent event arrives.
///
/// The fix is to reset the signal *before* the drain loop, so signals
/// raised during the drain are preserved and the next wait returns
/// immediately.
///
/// This test pins that invariant by driving the real `pull_events`
/// against a real `EvtSubscribe` handle. It installs a
/// `DRAIN_STEP_HOOK` that runs inside the drain loop after each
/// `EvtNext` and fires `SetEvent` on the subscription's signal
/// handle — simulating the OS signaling a new event arrival during
/// the drain window. After `pull_events` returns, the signal must
/// still be set — observed via a 0ms `wait_for_events_blocking`
/// so the check measures only the reset/preserve behavior of
/// `pull_events` and is not contaminated by unrelated Windows
/// system events arriving during a nonzero wait. Under the old
/// post-drain `ResetEvent` order, the hook's `SetEvent` would be
/// clobbered by the reset and the immediate poll would return
/// `Timeout` — which is exactly what #25194 reports.
#[tokio::test]
#[serial]
async fn test_pull_events_preserves_setevent_during_drain() {
    use std::sync::Arc as StdArc;

    let mut config = WindowsEventLogConfig::default();
    config.channels = vec!["Application".to_string()];
    config.read_existing_events = true;
    config.event_timeout_ms = 1000;

    let (checkpointer, _temp_dir) = create_test_checkpointer().await;

    let mut subscription = EventLogSubscription::new(&config, checkpointer, false)
        .await
        .expect("Subscription creation should succeed");

    // Capture THIS subscription's signal handle so the hook can scope
    // itself to this test. DRAIN_STEP_HOOK is a process-global, and
    // cargo runs tests in parallel by default; without handle-keying,
    // a concurrent test's pull_events could trigger our one-shot
    // hook first, flip `fired`, and SetEvent on the wrong handle.
    let target_signal_raw = subscription.first_channel_signal_raw();

    // Install the drain-loop hook: every EvtNext call inside
    // pull_events fires SetEvent on the subscription's signal
    // handle. This simulates the OS signaling a fresh event
    // mid-drain, which is exactly the race window #25194 exposes.
    // The hook only needs to fire once to prove the invariant; we
    // use an AtomicBool to keep it deterministic. The hook is keyed
    // to `target_signal_raw` so concurrent pull_events calls from
    // other tests no-op here.
    let fired = StdArc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let fired = StdArc::clone(&fired);
        let hook: StdArc<dyn Fn(HANDLE) + Send + Sync> = StdArc::new(move |signal: HANDLE| {
            if signal.0 as isize != target_signal_raw {
                return;
            }
            if !fired.swap(true, std::sync::atomic::Ordering::SeqCst) {
                unsafe {
                    let _ = SetEvent(signal);
                }
            }
        });
        *DRAIN_STEP_HOOK.lock().unwrap() = Some(hook);
    }

    // Drop-guard: clear the hook even if the test panics, so it
    // doesn't contaminate other tests in the same process.
    struct HookGuard;
    impl Drop for HookGuard {
        fn drop(&mut self) {
            *DRAIN_STEP_HOOK.lock().unwrap() = None;
        }
    }
    let _guard = HookGuard;

    // Drive pull_events with a very large budget so the drain
    // exits via ERROR_NO_MORE_ITEMS (channel_drained = true),
    // which is the path that ran the post-drain ResetEvent in the
    // old buggy code. Exiting via budget exhaustion would skip
    // that reset and cause this test to false-pass against the
    // pre-fix code.
    let _ = subscription.pull_events(usize::MAX).unwrap_or_default();

    assert!(
        fired.load(std::sync::atomic::Ordering::SeqCst),
        "drain-loop hook never ran — pull_events must call EvtNext \
         at least once even on an empty channel"
    );

    // Observe the signal state IMMEDIATELY with a 0ms wait. We want
    // to know whether pull_events's reset clobbered the hook's
    // SetEvent — NOT whether new real events arrive during some
    // wait window. A nonzero timeout against the live Application
    // channel lets arbitrary Windows system events re-signal us
    // and false-pass against the pre-fix code. 0ms = WaitForMultiple-
    // Objects returns the current state with no grace period, so
    // only the reset/preserve behavior of pull_events is measured.
    let result = subscription.wait_for_events_blocking(0);

    match result {
        WaitResult::EventsAvailable => {}
        WaitResult::Timeout => panic!(
            "signal set during the drain window was lost — this is the \
             lost-wakeup race from vectordotdev/vector#25194. \
             pull_events must call ResetEvent BEFORE draining, not after."
        ),
        WaitResult::Shutdown => panic!("unexpected shutdown"),
    }
}
