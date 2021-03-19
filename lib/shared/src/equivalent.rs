/// A related trait to `PartialEq`, `Equivalent` tests if two pieces
/// have equivalent values, without necessarily being identical. This is
/// used to test for events having the same values but potentially
/// different timestamps, without removing the ability to compare them
/// for exact equality.
pub trait Equivalent<Rhs: ?Sized = Self> {
    fn equivalent(&self, other: &Rhs) -> bool;
}

#[macro_export]
macro_rules! assert_equiv {
    ($left:expr, $right:expr, $message:expr) => {{
        use $crate::Equivalent as _;
        match (&($left), &($right)) {
            (left, right) => {
                if !left.equivalent(right) {
                    panic!(
                        "assertion failed: {}\n\n{}\n",
                        $message,
                        pretty_assertions::Comparison::new(left, right)
                    );
                }
            }
        }
    }};
    ($left:expr, $right:expr,) => {
        $crate::assert_equiv!($left, $right)
    };
    ($left:expr, $right:expr) => {
        $crate::assert_equiv!($left, $right, "`left.equivalent(right)`")
    };
}

#[macro_export]
macro_rules! impl_equivalent_as_eq {
    ($type:ty) => {
        impl $crate::Equivalent for $type {
            fn equivalent(&self, other: &Self) -> bool {
                self == other
            }
        }
    };
}

impl<T: Equivalent> Equivalent for Vec<T> {
    fn equivalent(&self, other: &Self) -> bool {
        if self.len() == other.len() {
            self.iter().zip(other.iter()).all(|(a, b)| a.equivalent(b))
        } else {
            false
        }
    }
}

impl<T: Equivalent> Equivalent for Option<T> {
    fn equivalent(&self, other: &Self) -> bool {
        match (self, other) {
            (None, None) => true,
            (Some(left), Some(right)) => left.equivalent(right),
            _ => false,
        }
    }
}

impl<R: Equivalent, E: Equivalent> Equivalent for Result<R, E> {
    fn equivalent(&self, other: &Self) -> bool {
        match (self, other) {
            (Ok(left), Ok(right)) => left.equivalent(right),
            (Err(left), Err(right)) => left.equivalent(right),
            _ => false,
        }
    }
}
