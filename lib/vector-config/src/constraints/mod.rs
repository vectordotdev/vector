#![allow(warnings)]

mod computed;
use std::{marker::PhantomData, borrow::Cow};

use serde_json::Value;

pub use self::computed::*;

mod path;
pub use self::path::InstancePath;

pub trait Constrainable {
    fn constraints(path: InstancePath) -> Constraints {
        Constraints::from_path(path)
    }
}

pub enum ConstraintData<'a, T: Clone> {
    Actual(Cow<'a, T>),
    Serialized(Cow<'a, Value>),
}

pub struct ConstraintTransformer<Input, Output>
where
    Input: Clone,
    Output: Clone,
{
    transformer: Box<dyn for<'a> Fn(&'a ConstraintData<'a, Input>) -> ConstraintData<'a, Output>>,
}

impl<Input, Output> ConstraintTransformer<Input, Output>
where
    Input: Clone,
    Output: Clone,
{
    pub fn new<FA: 'static, FS: 'static>(actual: FA, serialized: FS) -> Self
    where
        Input: 'static,
        Output: 'static,
        FA: for<'a> Fn(&'a Input) -> Cow<'a, Output>,
        FS: for<'a> Fn(&'a Value) -> Cow<'a, Value>,
    {
        Self {
            transformer: Box::new(move |input| transform(input, &actual, &serialized))
        }
    }

    pub fn transform<'a>(&self, input: &'a ConstraintData<'a, Input>) -> ConstraintData<'a, Output> {
        (self.transformer)(input)
    }
}

fn transform<'a, Input, Output, FA, FS, AO, SO>(input: &'a ConstraintData<'a, Input>, actual: &FA, serialized: &FS) -> ConstraintData<'a, Output>
where
    Input: Clone,
    Output: Clone,
    FA: Fn(&'a Input) -> AO,
    AO: Into<Cow<'a, Output>>,
    FS: Fn(&'a Value) -> SO,
    SO: Into<Cow<'a, Value>>,
{
    match input {
        ConstraintData::Actual(actual_input) => ConstraintData::Actual(actual(&actual_input).into()),
        ConstraintData::Serialized(serialized_input) => ConstraintData::Serialized(serialized(&serialized_input).into()),
    }
}
