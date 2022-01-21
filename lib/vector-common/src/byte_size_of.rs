use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
};

use serde_json::{value::RawValue, Value};

pub trait ByteSizeOf {
    /// Returns the in-memory size of this type
    ///
    /// This function returns the total number of bytes that
    /// [`std::mem::size_of`] does in addition to any interior allocated
    /// bytes. It default implementation is `std::mem::size_of` +
    /// `ByteSizeOf::allocated_bytes`.
    fn size_of(&self) -> usize {
        mem::size_of_val(self) + self.allocated_bytes()
    }

    /// Returns the allocated bytes of this type
    ///
    /// This function returns the total number of bytes that have been allocated
    /// interior to this type instance. It does not include any bytes that are
    /// captured by [`std::mem::size_of`] except for any bytes that are iterior
    /// to this type. For instance, `BTreeMap<String, Vec<u8>>` would count all
    /// bytes for `String` and `Vec<u8>` instances but not the exterior bytes
    /// for `BTreeMap`.
    fn allocated_bytes(&self) -> usize;
}

impl ByteSizeOf for String {
    fn allocated_bytes(&self) -> usize {
        self.len()
    }
}

impl<K, V> ByteSizeOf for BTreeMap<K, V>
where
    K: ByteSizeOf,
    V: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter()
            .fold(0, |acc, (k, v)| acc + k.size_of() + v.size_of())
    }
}

impl<T> ByteSizeOf for BTreeSet<T>
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter().map(ByteSizeOf::size_of).sum()
    }
}

impl<T> ByteSizeOf for Vec<T>
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter().map(ByteSizeOf::size_of).sum()
    }
}

impl<T> ByteSizeOf for &[T]
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter().map(ByteSizeOf::size_of).sum()
    }
}

impl<T> ByteSizeOf for Option<T>
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.as_ref().map_or(0, ByteSizeOf::allocated_bytes)
    }
}

macro_rules! num {
    ($t:ty) => {
        impl ByteSizeOf for $t {
            fn allocated_bytes(&self) -> usize {
                0
            }
        }
    };
}

num!(u8);
num!(u16);
num!(u32);
num!(u64);
num!(u128);
num!(i8);
num!(i16);
num!(i32);
num!(i64);
num!(i128);
num!(f32);
num!(f64);

impl ByteSizeOf for Box<RawValue> {
    fn allocated_bytes(&self) -> usize {
        self.get().len()
    }
}

impl ByteSizeOf for Value {
    fn allocated_bytes(&self) -> usize {
        match self {
            Value::Null | Value::Bool(_) | Value::Number(_) => 0,
            Value::String(s) => s.len(),
            Value::Array(a) => a.size_of(),
            Value::Object(o) => o.iter().map(|(k, v)| k.size_of() + v.size_of()).sum(),
        }
    }
}
