use crate::{
    config::{SourceConfig, SourceContext},
    sources,
};

use tokio::time;

use super::Sources;

#[derive(Clone, Debug)]
pub struct DemoMode {
    inner: Sources,
}

impl DemoMode {
    pub(crate) const fn new(source: Sources) -> Self {
        Self { inner: source }
    }

    pub(crate) fn build(&self, mut cx: SourceContext) -> sources::Source {
        let mut interval = time::interval(time::Duration::from_secs(1));
        let this = self.clone();

        Box::pin(async move {
            loop {
                tokio::select! {
                    _ = &mut cx.shutdown => break,
                    _ =  interval.tick() =>  {
                        let event = this.inner.generate_demo_data();
                        cx.out.send_event(event).await.unwrap();
                    }
                }
            }
            Ok(())
        })
    }
}

const MESSAGES: [&'static str; 5] = [
    "something happened",
    "all these things went wrong",
    "ohno look what went down here",
    "this is a great log message",
    "i find this all very interesting",
];

/// Returns a random message from a preset list of messages.
pub fn random_message() -> &'static str {
    MESSAGES[rand::random::<usize>() % 5]
}
