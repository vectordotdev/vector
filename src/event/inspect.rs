use crate::event::{Event, LogEvent};
use parking_lot::RwLock;
use std::collections::HashMap;
use tokio::sync::broadcast;

type Result = ::std::result::Result<Event, ()>;
type Sender = broadcast::Sender<LogEvent>;
type Receiver = broadcast::Receiver<LogEvent>;

pub struct EventInspector {
    components: RwLock<HashMap<String, Sender>>,
}

impl EventInspector {
    pub fn new() -> Self {
        Self {
            components: RwLock::new(HashMap::new()),
        }
    }

    /// Associates a broadcast::Sender with a component name, so that subscribers may selectively
    /// receive `LogEvent`s based on specific components. Returns a function that can be provided
    /// to `.inspect` on an upstream LogEvent receiver channel.
    pub fn adder(&self, component_name: String) -> impl FnMut(&Event) {
        let mut lock = self.components.write();
        let tx = lock.entry(component_name.clone()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(100);
            tx
        });

        // Clone the sender, to move an owned copy into the returning function
        let tx = tx.clone();

        move |ev| {
            match ev {
                Event::Log(ev) => {
                    // broadcast::Sender provides an atomic count of the number of active
                    // listeners. We're using that here to avoid a potentally expensive clone
                    // in the (likely) event that there are no current subscribers.
                    //
                    // This can suffer from TOC/TOU, but the risk is minimal as the purpose
                    // here is solely to reduce expense.
                    if tx.receiver_count() > 0 {
                        let _ = tx.send(ev.clone());
                    }
                }
                _ => {}
            }
        }
    }

    /// Version of `adder` that returns a func for inspecting Result<Event, ()>
    pub fn result_adder(&self, component_name: String) -> impl FnMut(&Result) {
        let mut func = self.adder(component_name);

        move |r| {
            if let Ok(ev) = r {
                func(&ev)
            }
        }
    }

    /// Subscribe to `LogEvent`s received against a specific component name. Any additional
    /// filtering should be done downstream
    pub fn subscribe(&self, component_name: &str) -> Option<Receiver> {
        self.components
            .read()
            .get(component_name)
            .map(|tx| tx.subscribe())
    }
}
