use crate::Event;
use snafu::Snafu;

pub mod util;

#[cfg(feature = "transforms-add_fields")]
pub mod add_fields;
#[cfg(feature = "transforms-add_tags")]
pub mod add_tags;
#[cfg(feature = "transforms-ansi_stripper")]
pub mod ansi_stripper;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
pub mod aws_cloudwatch_logs_subscription_parser;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub mod aws_ec2_metadata;
#[cfg(feature = "transforms-coercer")]
pub mod coercer;
#[cfg(feature = "transforms-concat")]
pub mod concat;
#[cfg(feature = "transforms-dedupe")]
pub mod dedupe;
#[cfg(feature = "transforms-field_filter")]
pub mod field_filter;
#[cfg(feature = "transforms-filter")]
pub mod filter;
#[cfg(feature = "transforms-geoip")]
pub mod geoip;
#[cfg(feature = "transforms-grok_parser")]
pub mod grok_parser;
#[cfg(feature = "transforms-json_parser")]
pub mod json_parser;
#[cfg(feature = "transforms-key_value_parser")]
pub mod key_value_parser;
#[cfg(feature = "transforms-kubernetes_metadata")]
pub mod kubernetes_metadata;
#[cfg(feature = "transforms-log_to_metric")]
pub mod log_to_metric;
#[cfg(feature = "transforms-logfmt_parser")]
pub mod logfmt_parser;
#[cfg(feature = "transforms-lua")]
pub mod lua;
#[cfg(feature = "transforms-merge")]
pub mod merge;
#[cfg(feature = "transforms-metric_to_log")]
pub mod metric_to_log;
#[cfg(feature = "transforms-reduce")]
pub mod reduce;
#[cfg(feature = "transforms-regex_parser")]
pub mod regex_parser;
#[cfg(feature = "transforms-remap")]
pub mod remap;
#[cfg(feature = "transforms-remove_fields")]
pub mod remove_fields;
#[cfg(feature = "transforms-remove_tags")]
pub mod remove_tags;
#[cfg(feature = "transforms-rename_fields")]
pub mod rename_fields;
#[cfg(feature = "transforms-sampler")]
pub mod sampler;
#[cfg(feature = "transforms-split")]
pub mod split;
#[cfg(feature = "transforms-swimlanes")]
pub mod swimlanes;
#[cfg(feature = "transforms-tag_cardinality_limit")]
pub mod tag_cardinality_limit;
#[cfg(feature = "transforms-tokenizer")]
pub mod tokenizer;
#[cfg(feature = "wasm")]
pub mod wasm;

/// Transforms come in two variants. Functions, or tasks.
///
/// While function transforms can be run out of order, or concurrently, task transforms act as a coordination or barrier point.
pub enum Transform {
    Function(Box<dyn FunctionTransform>),
    Task(Box<dyn TaskTransform>),
}

impl Transform {
    /// Create a new function transform.
    ///
    /// These functions are "stateless" and can be run in parallel, without regard for coordination.
    ///
    /// **Note:** You should prefer to implement this over [`TaskTransform`] where possible.
    pub fn function(v: impl FunctionTransform + 'static) -> Self {
        Transform::Function(Box::new(v))
    }
    /// Mutably borrow the inner transform as a function transform.
    ///
    /// # Panics
    ///
    /// If the transform is a [`TaskTransform`] this will panic.
    pub fn as_function(&mut self) -> &mut Box<dyn FunctionTransform> {
        match self {
            Transform::Function(t) => t,
            Transform::Task(_) => panic!(
                "Called `Transform::as_function` on something that was not a function variant."
            ),
        }
    }
    /// Transmute the inner transform into a function transform.
    ///
    /// # Panics
    ///
    /// If the transform is a [`TaskTransform`] this will panic.
    pub fn into_function(self) -> Box<dyn FunctionTransform> {
        match self {
            Transform::Function(t) => t,
            Transform::Task(_) => panic!(
                "Called `Transform::into_function` on something that was not a function variant."
            ),
        }
    }
    /// Create a new task transform.
    ///
    /// These tasks are coordinated, and map a stream of some `U` to some other `T`.
    ///
    /// **Note:** You should prefer to implement [`FunctionTransform`] over this where possible.
    pub fn task(v: impl TaskTransform + 'static) -> Self {
        Transform::Task(Box::new(v))
    }
    /// Mutably borrow the inner transform as a task transform.
    ///
    /// # Panics
    ///
    /// If the transform is a [`FunctionTransform`] this will panic.
    pub fn as_task(&mut self) -> &mut Box<dyn TaskTransform> {
        match self {
            Transform::Function(_) => {
                panic!("Called `Transform::as_task` on something that was not a task variant.")
            }
            Transform::Task(t) => t,
        }
    }
    /// Transmute the inner transform into a task transform.
    ///
    /// # Panics
    ///
    /// If the transform is a [`FunctionTransform`] this will panic.
    pub fn into_task(self) -> Box<dyn TaskTransform> {
        match self {
            Transform::Function(_) => {
                panic!("Called `Transform::into_task` on something that was not a task variant.")
            }
            Transform::Task(t) => t,
        }
    }
}

/// Transforms that are simple, and don't require attention to coordination.
/// You can run them as simple functions over events in any order.
///
/// # Invariants
///
/// * It is an illegal invariant to implement `FunctionTransform` for a `TaskTransform` or vice versa.
pub trait FunctionTransform: Send + dyn_clone::DynClone + Sync {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event);

    /// A handy test function that inputs and outputs only one event.
    ///
    /// In a prior time, Vector primarily used this API to handle events.
    /// However, it's now customary to only implement `transform` which handles multiple output events and can
    /// have it's allocation more effectively controlled.
    #[cfg_attr(
        not(test),
        deprecated = "Use `transform` and `output.extend(events)` or `output.push(event)` instead."
    )]
    fn transform_one(&mut self, event: Event) -> Option<Event> {
        let mut buf = Vec::with_capacity(1);
        self.transform(&mut buf, event);
        buf.into_iter().next()
    }
}

dyn_clone::clone_trait_object!(FunctionTransform);

/// Transforms that tend to be more complicated runtime style components.
///
/// These require coordination and map a stream of some `T` to some `U`.
///
/// # Invariants
///
/// * It is an illegal invariant to implement `FunctionTransform` for a `TaskTransform` or vice versa.
pub trait TaskTransform: Send {
    fn transform(
        self: Box<Self>,
        task: Box<dyn futures01::Stream<Item = Event, Error = ()> + Send>,
    ) -> Box<dyn futures01::Stream<Item = Event, Error = ()> + Send>
    where
        Self: 'static;
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid regular expression: {}", source))]
    InvalidRegex { source: regex::Error },

    #[snafu(display("Invalid substring expression: {}", name))]
    InvalidSubstring { name: String },
}
