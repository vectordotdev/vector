use futures01::{Future, Poll};
use std::fmt;

/// High level topology task.
pub struct Task {
    inner: Box<dyn Future<Item = (), Error = ()> + Send + 'static>,
    name: String,
    typetag: String,
}

impl Task {
    pub fn new(
        name: &str,
        typetag: &str,
        inner: impl Future<Item = (), Error = ()> + Send + 'static,
    ) -> Self {
        Self {
            inner: Box::new(inner),
            name: name.into(),
            typetag: typetag.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn typetag(&self) -> &str {
        &self.typetag
    }
}

impl Future for Task {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll()
    }
}

impl fmt::Debug for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Task")
            .field("name", &self.name)
            .field("typetag", &self.typetag)
            .finish()
    }
}
