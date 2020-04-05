pub mod hostcalls;

pub trait ForeignTransform<AbstractEvent, AbstractError>: Sized {
    fn process(&mut self, event: AbstractEvent) -> Result<Option<AbstractEvent>, AbstractError>;
}
