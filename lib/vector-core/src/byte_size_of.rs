// use std::{
//     collections::{BTreeMap, BTreeSet},
//     mem,
// };

// use core_common::byte_size_of::ByteSizeOf;

// pub trait ByteSizeOf {
//     /// Returns the in-memory size of this type
//     ///
//     /// This function returns the total number of bytes that
//     /// [`std::mem::size_of`] does in addition to any interior allocated
//     /// bytes. It default implementation is `std::mem::size_of` +
//     /// `ByteSizeOf::allocated_bytes`.
//     fn size_of(&self) -> usize {
//         mem::size_of_val(self) + self.allocated_bytes()
//     }

//     /// Returns the allocated bytes of this type
//     ///
//     /// This function returns the total number of bytes that have been allocated
//     /// interior to this type instance. It does not include any bytes that are
//     /// captured by [`std::mem::size_of`] except for any bytes that are iterior
//     /// to this type. For instance, `BTreeMap<String, Vec<u8>>` would count all
//     /// bytes for `String` and `Vec<u8>` instances but not the exterior bytes
//     /// for `BTreeMap`.
//     fn allocated_bytes(&self) -> usize;
// }
