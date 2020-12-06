use crate::{
    transforms::{FunctionTransform, TaskTransform, Transform},
    Event,
};
use futures01::{Poll, Stream};
use std::cell::Cell;

thread_local!(static WRAPPED: Cell<bool> = Cell::new(false));

struct WrapToken<'a> {
    cell: &'a Cell<bool>,
    before: bool,
}

impl<'a> Drop for WrapToken<'a> {
    fn drop(&mut self) {
        self.cell.set(self.before);
    }
}

/// Wraps function so that inside of it fn is_wrapped returns true.
pub fn wrap<R>(f: impl FnOnce() -> R) -> R {
    WRAPPED.with(|wrapped| {
        let _token = WrapToken {
            cell: wrapped,
            before: wrapped.replace(true),
        };
        f()
    })
}

/// True if called from inside one or more fn wrap.
pub fn is_wrapped() -> bool {
    WRAPPED.with(|wrapped| wrapped.get())
}

// ****** Convenient wrappers ********* //

pub trait WrapEmit {
    /// Changes componenet to emit wrapped internal metrics.
    fn wrap_emit(self) -> Self;
}

impl WrapEmit for Transform {
    fn wrap_emit(self) -> Self {
        match self {
            Self::Function(t) => Self::Function(t.wrap_emit()),
            Self::Task(t) => Self::Task(t.wrap_emit()),
        }
    }
}

impl WrapEmit for Box<dyn FunctionTransform> {
    fn wrap_emit(self) -> Self {
        Box::new(FunctionTransformEmitWrapper::new(self))
    }
}

impl WrapEmit for Box<dyn TaskTransform> {
    fn wrap_emit(self) -> Self {
        Box::new(TaskTransformEmitWrapper::new(self))
    }
}

impl WrapEmit for Box<dyn Stream<Item = Event, Error = ()> + Send> {
    fn wrap_emit(self) -> Self {
        Box::new(StreamEmitWrapper::new(self))
    }
}

pub struct FunctionTransformEmitWrapper<F: FunctionTransform + ?Sized>(Box<F>);

impl<F: FunctionTransform + ?Sized> FunctionTransformEmitWrapper<F> {
    pub fn new(function: Box<F>) -> Self {
        Self(function)
    }
}

impl<F: FunctionTransform + ?Sized> FunctionTransform for FunctionTransformEmitWrapper<F> {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        wrap(|| self.0.transform(output, event));
    }
}

impl<F: FunctionTransform + ?Sized> Clone for FunctionTransformEmitWrapper<F> {
    fn clone(&self) -> Self {
        Self(dyn_clone::clone_box(&*self.0))
    }
}

pub struct TaskTransformEmitWrapper<F: TaskTransform + ?Sized>(Box<F>);

impl<F: TaskTransform + ?Sized> TaskTransformEmitWrapper<F> {
    pub fn new(task: Box<F>) -> Self {
        Self(task)
    }
}

impl<F: TaskTransform + ?Sized> TaskTransform for TaskTransformEmitWrapper<F> {
    fn transform(
        self: Box<Self>,
        task: Box<dyn Stream<Item = Event, Error = ()> + Send>,
    ) -> Box<dyn Stream<Item = Event, Error = ()> + Send>
    where
        Self: 'static,
    {
        self.0.transform(task).wrap_emit()
    }
}

pub struct StreamEmitWrapper<S: Stream>(S);

impl<S: Stream> StreamEmitWrapper<S> {
    pub fn new(stream: S) -> Self {
        Self(stream)
    }
}

impl<S: Stream> Stream for StreamEmitWrapper<S> {
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        wrap(|| self.0.poll())
    }
}
