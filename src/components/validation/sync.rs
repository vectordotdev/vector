use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use tokio::sync::{oneshot, Notify};

struct WaitGroupState {
    registered: AtomicUsize,
    done: AtomicUsize,
    notify: Notify,
}

impl WaitGroupState {
    fn all_children_done(&self) -> bool {
        self.registered.load(Ordering::Acquire) == self.done.load(Ordering::Acquire)
    }
}
/// A synchronization primitive for waiting for an arbitrary number of processes to rendezvous.
pub struct WaitGroup {
    locked: bool,
    state: Arc<WaitGroupState>,
}

pub struct WaitGroupChild {
    done: bool,
    state: Arc<WaitGroupState>,
}

impl WaitGroup {
    /// Creates a new `WaitGroup`.
    pub fn new() -> Self {
        Self {
            locked: false,
            state: Arc::new(WaitGroupState {
                registered: AtomicUsize::new(0),
                done: AtomicUsize::new(0),
                notify: Notify::new(),
            }),
        }
    }

    /// Creates and attaches a new child to this wait group.
    ///
    /// ## Panics
    ///
    /// If the caller attempts to add a child after calling `wait_for_children` at least once, this
    /// method will panic.
    pub fn add_child(&self) -> WaitGroupChild {
        if self.locked {
            panic!("tried to add child after wait group locked");
        }

        WaitGroupChild::from_state(&self.state)
    }

    /// Waits until all children have marked themselves as done.
    ///
    /// If no children were added to the wait group, or all of them have already completed, this
    /// function returns immediately.
    pub async fn wait_for_children(&mut self) {
        // We "lock" ourselves because, if we did not, then _technically_ we can't be sure that caller
        // hasn't called this method multiple times, after a new child being added in between...
        // which messes up the invariant that once we start waiting, nothing else should be added.
        //
        // It's easier to do that internally, and panic if `add_child` is called after the first
        // call to `wait_for_children`, rather than deal with having to make this future
        // cancellation safe some other way.
        if !self.locked {
            self.locked = true;
        }

        while !self.state.all_children_done() {
            self.state.notify.notified().await;
        }
    }
}

impl WaitGroupChild {
    fn from_state(state: &Arc<WaitGroupState>) -> Self {
        state.registered.fetch_add(1, Ordering::AcqRel);

        Self {
            done: false,
            state: Arc::clone(state),
        }
    }

    /// Marks this child as done.
    ///
    /// If the wait group has been finalized and is waiting for all children to be marked as done,
    /// and this is the last outstanding child to be marked as done, the wait group will be notified.
    pub fn mark_as_done(mut self) {
        self.done = true;

        self.state.done.fetch_add(1, Ordering::SeqCst);
        if self.state.all_children_done() {
            self.state.notify.notify_one();
        }
    }
}

impl Drop for WaitGroupChild {
    fn drop(&mut self) {
        if !self.done {
            panic!("wait group child dropped without being marked as done");
        }
    }
}

pub struct WaitTrigger {
    tx: oneshot::Sender<()>,
}

pub struct WaitHandle {
    rx: Option<oneshot::Receiver<()>>,
}

impl WaitTrigger {
    /// Creates a new waiter pair.
    pub fn new() -> (Self, WaitHandle) {
        let (tx, rx) = oneshot::channel();

        let trigger = Self { tx };
        let handle = WaitHandle { rx: Some(rx) };

        (trigger, handle)
    }

    /// Triggers the wait handle to wake up.
    pub fn trigger(self) {
        // We don't care if our trigger is actually received, because the receiver side may
        // intentionally not be used i.e. if the code is generic in a way where only some codepaths
        // wait to be triggered and others don't, but the trigger must always be called regardless.
        _ = self.tx.send(());
    }
}

impl WaitHandle {
    /// Waits until triggered.
    pub async fn wait(&mut self) {
        match self.rx.as_mut() {
            Some(rx) => rx
                .await
                .expect("paired task no longer holding wait trigger"),
            None => panic!("tried to await wait trigger signal but has already been received"),
        }

        // If we're here, we've successfully received the signal, so we consume the
        // receiver, as it cannot be used/polled again.
        self.rx.take();
    }
}

pub struct Configuring {
    tasks_started: WaitGroup,
    tasks_completed: WaitGroup,
    shutdown_triggers: Mutex<Vec<WaitTrigger>>,
}

pub struct Started {
    tasks_completed: Option<WaitGroup>,
    shutdown_triggers: Vec<WaitTrigger>,
}

/// Coordination primitive for external tasks.
///
/// When validating a component, an external resource may be spun up either to provide the inputs to
/// the component or to act as the collector of outputs from the component. Additionally, other
/// tasks may be spawned to forward data between parts of the topology. The validation runner must
/// be able to ensure that these tasks have started, and completed, at different stages of the
/// validation run, to ensure all inputs have been processed, or that all outputs have been received.
///
/// This coordinator uses a state machine that is encoded into the type of the coordinator itself to
/// ensure that once it has begin configured -- tasks are registered -- that it can only be used in
/// a forward direction: waiting for all tasks to start, and after that, signalling all tasks to
/// shutdown and waiting for them to do so.
///
/// This approach provides a stronger mechanism for avoiding bugs such as adding registered tasks
/// after waiting for all tasks to start, and so on.
pub struct TaskCoordinator<State> {
    state: State,
}

impl TaskCoordinator<()> {
    /// Creates a new `TaskCoordinator`.
    pub fn new() -> TaskCoordinator<Configuring> {
        TaskCoordinator {
            state: Configuring {
                tasks_started: WaitGroup::new(),
                tasks_completed: WaitGroup::new(),
                shutdown_triggers: Mutex::new(Vec::new()),
            },
        }
    }
}

impl TaskCoordinator<Configuring> {
    /// Attaches a new child to the wait group that tracks when tasks have started.
    pub fn track_started(&self) -> WaitGroupChild {
        self.state.tasks_started.add_child()
    }

    /// Attaches a new child to the wait group that tracks when tasks have completed.
    pub fn track_completed(&self) -> WaitGroupChild {
        self.state.tasks_completed.add_child()
    }

    /// Registers a handle that will be notified when shutdown is triggered.
    pub fn register_for_shutdown(&self) -> WaitHandle {
        let (trigger, handle) = WaitTrigger::new();
        self.state
            .shutdown_triggers
            .lock()
            .expect("poisoned")
            .push(trigger);
        handle
    }

    /// Waits for all tasks to have marked that they have started.
    pub async fn started(self) -> TaskCoordinator<Started> {
        let Configuring {
            mut tasks_started,
            tasks_completed,
            shutdown_triggers,
        } = self.state;

        tasks_started.wait_for_children().await;
        trace!("All coordinated tasks reported as having started.");

        TaskCoordinator {
            state: Started {
                tasks_completed: Some(tasks_completed),
                shutdown_triggers: shutdown_triggers.into_inner().expect("poisoned"),
            },
        }
    }
}

impl TaskCoordinator<Started> {
    /// Triggers all coordinated tasks to shutdown, and waits for them to mark themselves as completed.
    pub async fn shutdown(mut self) {
        // Trigger all registered shutdown handles.
        for trigger in self.state.shutdown_triggers.drain(..) {
            trigger.trigger();
            trace!("Shutdown triggered for coordinated tasks.");
        }

        // Now simply wait for all of them to mark themselves as completed.
        trace!("Waiting for coordinated tasks to complete...");
        let tasks_completed = self
            .state
            .tasks_completed
            .as_mut()
            .expect("tasks completed wait group already consumed");
        tasks_completed.wait_for_children().await;
        trace!("All coordinated tasks completed.");
    }
}
