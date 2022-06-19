use core::Resolved;
use std::{cell::RefCell, rc::Rc};
use vector_common::TimeZone;

use crate::{state::Runtime, Target};

pub struct Context<'a> {
    target: &'a mut dyn Target,
    state: &'a mut Runtime,
    timezone: &'a TimeZone,
}

impl<'a> Context<'a> {
    /// Create a new [`Context`].
    pub fn new(target: &'a mut dyn Target, state: &'a mut Runtime, timezone: &'a TimeZone) -> Self {
        Self {
            target,
            state,
            timezone,
        }
    }

    /// Get a reference to the [`Target`].
    pub fn target(&self) -> &dyn Target {
        self.target
    }

    /// Get a mutable reference to the [`Target`].
    pub fn target_mut(&mut self) -> &mut dyn Target {
        self.target
    }

    /// Get a reference to the [`runtime state`](Runtime).
    pub fn state(&self) -> &Runtime {
        self.state
    }

    /// Get a mutable reference to the [`runtime state`](Runtime).
    pub fn state_mut(&mut self) -> &mut Runtime {
        self.state
    }

    /// Get a reference to the [`TimeZone`]
    pub fn timezone(&self) -> &TimeZone {
        self.timezone
    }
}

#[derive(Clone)]
pub struct BatchContext {
    resolved_values: Vec<Resolved>,
    targets: Vec<Rc<RefCell<dyn Target>>>,
    states: Vec<Rc<RefCell<Runtime>>>,
    timezone: TimeZone,
}

impl BatchContext {
    /// Create a new [`BatchContext`].
    pub fn new(
        resolved_values: Vec<Resolved>,
        targets: Vec<Rc<RefCell<dyn Target>>>,
        states: Vec<Rc<RefCell<Runtime>>>,
        timezone: TimeZone,
    ) -> Self {
        Self {
            resolved_values,
            targets,
            states,
            timezone,
        }
    }

    pub fn empty_with_timezone(timezone: TimeZone) -> Self {
        Self {
            resolved_values: Vec::new(),
            targets: Vec::new(),
            states: Vec::new(),
            timezone,
        }
    }

    pub fn len(&self) -> usize {
        self.resolved_values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.resolved_values.is_empty()
    }

    pub fn resolved_values_mut(&mut self) -> &mut Vec<Resolved> {
        &mut self.resolved_values
    }

    pub fn targets(&self) -> impl Iterator<Item = Rc<RefCell<dyn Target>>> + '_ {
        self.targets.iter().cloned()
    }

    pub fn states(&self) -> impl Iterator<Item = Rc<RefCell<Runtime>>> + '_ {
        self.states.iter().cloned()
    }

    pub fn drain_filter<F>(&mut self, mut filter: F) -> Self
    where
        F: FnMut(&mut Resolved) -> bool,
    {
        let mut this_resolved_values = Vec::new();
        let mut this_targets = Vec::new();
        let mut this_states = Vec::new();
        let mut other_resolved_values = Vec::new();
        let mut other_targets = Vec::new();
        let mut other_states = Vec::new();

        std::mem::swap(&mut self.resolved_values, &mut this_resolved_values);
        std::mem::swap(&mut self.targets, &mut this_targets);
        std::mem::swap(&mut self.states, &mut this_states);

        for ((mut resolved, target), state) in this_resolved_values
            .into_iter()
            .zip(this_targets)
            .zip(this_states)
        {
            if filter(&mut resolved) {
                other_resolved_values.push(resolved);
                other_targets.push(target);
                other_states.push(state);
            } else {
                self.resolved_values.push(resolved);
                self.targets.push(target);
                self.states.push(state);
            }
        }

        Self {
            resolved_values: other_resolved_values,
            targets: other_targets,
            states: other_states,
            timezone: self.timezone,
        }
    }

    pub fn filtered<P>(self, mut predicate: P) -> Self
    where
        P: FnMut(&Resolved) -> bool,
    {
        let ((resolved_values, targets), states) = self
            .resolved_values
            .into_iter()
            .zip(self.targets.into_iter())
            .zip(self.states.into_iter())
            .filter(|((value, _), _)| predicate(value))
            .unzip();

        Self {
            resolved_values,
            targets,
            states,
            timezone: self.timezone,
        }
    }

    pub fn extend(&mut self, other: BatchContext) {
        assert_eq!(self.timezone, other.timezone);
        self.resolved_values.extend(other.resolved_values);
        self.targets.extend(other.targets);
        self.states.extend(other.states);
    }

    pub fn timezone(&self) -> TimeZone {
        self.timezone
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl Iterator<
        Item = (
            &'_ mut Resolved,
            Rc<RefCell<dyn Target>>,
            Rc<RefCell<Runtime>>,
            TimeZone,
        ),
    > {
        let resolved_values = self.resolved_values.iter_mut();
        let targets = self.targets.iter();
        let states = self.states.iter();
        let timezone = self.timezone;

        BatchContextIterMut {
            resolved_values,
            targets,
            states,
            timezone,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn into_parts(
        self,
    ) -> (
        Vec<Resolved>,
        Vec<Rc<RefCell<dyn Target>>>,
        Vec<Rc<RefCell<Runtime>>>,
        TimeZone,
    ) {
        (
            self.resolved_values,
            self.targets,
            self.states,
            self.timezone,
        )
    }
}

pub struct BatchContextIterMut<'a> {
    resolved_values: std::slice::IterMut<'a, Resolved>,
    targets: std::slice::Iter<'a, Rc<RefCell<dyn Target>>>,
    states: std::slice::Iter<'a, Rc<RefCell<Runtime>>>,
    timezone: TimeZone,
}

impl<'a> Iterator for BatchContextIterMut<'a> {
    type Item = (
        &'a mut Resolved,
        Rc<RefCell<dyn Target>>,
        Rc<RefCell<Runtime>>,
        TimeZone,
    );

    fn next(&mut self) -> Option<Self::Item> {
        Some((
            self.resolved_values.next()?,
            self.targets.next()?.clone(),
            self.states.next()?.clone(),
            self.timezone,
        ))
    }
}
