use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
};

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
        self.iter().fold(0, |acc, v| acc + v.size_of())
    }
}

impl<T> ByteSizeOf for Vec<T>
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.iter().fold(0, |acc, v| acc + v.size_of())
    }
}

impl<T> ByteSizeOf for Option<T>
where
    T: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.as_ref().map_or(0, |x| x.allocated_bytes())
    }
}
