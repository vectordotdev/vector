/// A related trait to `PartialEq`, `EventDataEq` tests if two events
/// contain the same data, exclusive of the metadata. This is used to
/// test for events having the same values but potentially different
/// parts of the metadata that not fixed between runs, without removing
/// the ability to compare them for exact equality.
pub trait EventDataEq<Rhs: ?Sized = Self> {
    fn event_data_eq(&self, other: &Rhs) -> bool;
}

#[macro_export]
macro_rules! assert_event_data_eq {
    ($left:expr, $right:expr, $message:expr) => {{
        use $crate::EventDataEq as _;
        match (&($left), &($right)) {
            (left, right) => {
                assert!(
                    left.event_data_eq(right),
                    "assertion failed: {}\n\n{}\n",
                    $message,
                    pretty_assertions::Comparison::new(left, right),
                );
            }
        }
    }};
    ($left:expr, $right:expr,) => {
        $crate::assert_event_data_eq!($left, $right)
    };
    ($left:expr, $right:expr) => {
        $crate::assert_event_data_eq!($left, $right, "`left.event_data_eq(right)`")
    };
}

#[macro_export]
macro_rules! impl_event_data_eq {
    ($type:ty) => {
        impl $crate::EventDataEq for $type {
            fn event_data_eq(&self, other: &Self) -> bool {
                self == other
            }
        }
    };
}

impl<T: EventDataEq> EventDataEq for &[T] {
    fn event_data_eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .zip(other.iter())
                .all(|(a, b)| a.event_data_eq(b))
    }
}

impl<T: EventDataEq> EventDataEq for Vec<T> {
    fn event_data_eq(&self, other: &Self) -> bool {
        self.as_slice().event_data_eq(&other.as_slice())
    }
}

impl<T: EventDataEq> EventDataEq for Option<T> {
    fn event_data_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (None, None) => true,
            (Some(left), Some(right)) => left.event_data_eq(right),
            _ => false,
        }
    }
}

impl<R: EventDataEq, E: EventDataEq> EventDataEq for Result<R, E> {
    fn event_data_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Ok(left), Ok(right)) => left.event_data_eq(right),
            (Err(left), Err(right)) => left.event_data_eq(right),
            _ => false,
        }
    }
}
