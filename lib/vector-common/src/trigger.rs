use stream_cancel::Trigger;

#[allow(clippy::module_name_repetitions)]
pub struct DisabledTrigger {
    trigger: Option<Trigger>,
}

impl DisabledTrigger {
    #[must_use]
    pub fn new(t: Trigger) -> Self {
        Self { trigger: Some(t) }
    }

    #[must_use]
    pub fn into_inner(mut self) -> Trigger {
        self.trigger.take().unwrap_or_else(|| unreachable!())
    }
}

impl Drop for DisabledTrigger {
    fn drop(&mut self) {
        if let Some(trigger) = self.trigger.take() {
            trigger.disable();
        }
    }
}

impl From<Trigger> for DisabledTrigger {
    fn from(t: Trigger) -> Self {
        Self::new(t)
    }
}
