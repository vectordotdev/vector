use crate::internal_events::InternalReloadFailed;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Generation counter incremented for each reload.
static GENERATION: AtomicUsize = AtomicUsize::new(0);

/// Increments generation causing all Ages to report that they
/// are old.
pub(super) fn inc_generation() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Age to be paired with some relodable data.
///
/// Usually used through more generic wrapper
/// structures, but can be used in custom structs
/// where this general procedure should be followed:
/// 1. Age::new()
/// 2. create data
/// 3. if Age::is_old() continue from 6.
/// 4. reload data
/// 5. if Age::set_age() is old repeat from 4.
/// 6. use data
///
/// Start from 1. when first creating data.
/// Start from 3. when accesing data.
///
/// Or update method can be used.
#[derive(Debug, PartialEq, Eq)]
pub struct Age {
    gen: usize,
}

impl Age {
    /// Creates new Age with current generation.
    pub fn new() -> Self {
        Age {
            gen: GENERATION.load(Ordering::Relaxed),
        }
    }

    /// Returns new age to be seted after the data
    /// has been reloaded.
    pub fn is_old(&self) -> Option<Age> {
        let current = Self::new();
        if current != *self {
            Some(current)
        } else {
            None
        }
    }

    /// Sets age to new value and checks wheter this new
    /// age is old in which case it returns even newer age.
    pub fn set_age(&mut self, new: Age) -> Option<Age> {
        *self = new;
        self.is_old()
    }

    /// Returns updated data.
    ///
    /// Reloads data when it's old and on failure to do so
    /// it reports warning and returns None.
    pub fn update<T>(&mut self, update: impl Fn() -> crate::Result<T>) -> Option<T> {
        let mut new = self.is_old();
        let mut data = None;
        while let Some(age) = new {
            data = update()
                .map_err(|error| emit!(InternalReloadFailed { error }))
                .ok()
                .or(data);
            new = self.set_age(age);
        }
        data
    }
}

/// Wrapper for relodable data.
///
/// Reloads data when it's old and on failure to do so
/// it reports warning and reuses old data.
pub struct Aged<T> {
    create: Box<dyn Fn() -> crate::Result<T> + Send>,
    data: T,
    age: Age,
}

impl<T> Aged<T> {
    pub fn new(create: impl Fn() -> crate::Result<T> + Send + 'static) -> crate::Result<Self> {
        let age = Age::new();
        let data = create()?;
        Ok(Self {
            create: Box::new(create) as Box<_>,
            data,
            age,
        })
    }

    pub fn as_ref(&mut self) -> &T {
        if let Some(data) = self.age.update(&self.create) {
            self.data = data;
        }

        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn reload() {
        let cell = Cell::new(0);
        let mut aged = Aged::new(move || {
            let new = cell.get() + 1;
            cell.set(new);
            Ok(new)
        })
        .unwrap();

        let start = *aged.as_ref();
        inc_generation();
        let new = *aged.as_ref();

        assert!(start < new);
    }

    #[test]
    fn reuse() {
        let cell = Cell::new(0);
        let mut aged = Aged::new(move || {
            let new = cell.get() + 1;
            cell.set(new);
            Ok(new)
        })
        .unwrap();

        let mut gen = aged.age.gen;
        loop {
            let first = *aged.as_ref();
            let second = *aged.as_ref();
            if aged.age.gen == gen {
                assert_eq!(first, second);
                break;
            }
            gen = aged.age.gen;
        }
    }
}
