use std::marker::PhantomData;
#[cfg(test)]
use std::pin::Pin;

use bytecheck::CheckBytes;
use rkyv::{
    Archive, Serialize, archived_root, check_archived_root,
    ser::{Serializer, serializers::AllocSerializer},
    validation::validators::DefaultValidator,
};

use super::ser::{DeserializeError, SerializeError};

/// A batteries-included serializer implementation.
///
/// Callers do not need to know or care about this, but it must be public as it is part of the
/// `BackedArchive` API.
pub type DefaultSerializer = AllocSerializer<4096>;

/// Backed wrapper for any type that implements [`Archive`][archive].
///
/// For any backing store that can provide references to an underlying byte slice of suitable size,
/// we can deserialize and serialize a type that is archivable. `BackedArchive` provides specific
/// entrypoints to either deserialize the given type from the backing store, or to serialize a
/// provided value to the backing store.
///
/// Once wrapped, the archived type can be accessed either immutably or mutably.  This provides a
/// simple mechanism to use a variety of backing stores, such as `Vec<u8>` or a memory-mapped
/// region.  This can in turn be used to avoid serializing to intermediate buffers when possible.
///
/// ## Archived types
///
/// Traditionally, (de)serialization frameworks focus on taking some type `T`, and translating it to
/// and from another format: structured text like JSON, or maybe binary data for efficient
/// on-the-wire representation, like Protocol Buffers.  `rkyv` works slightly differently, as a
/// zero-copy (de)serialization framework, by providing a projected type, or "archive", over the
/// underlying byte representation of `T`.
///
/// In general, what this means is that when you derive the correct traits for some type `T`, `rkyv`
/// generates an `ArchivedT` type that can correctly represent `T` when serialized to disk,
/// regardless of whether `T` contains primitive types or types holding underlying allocations, like
/// `Vec<T>`.
///
/// Crucially, the archive type -- `ArchivedT` -- can be pointer casted against the underlying bytes
/// to provide a reference of `ArchivedT`, or even a mutable reference.  This means that we can
/// access an object that is like our `T`, in a native and ergonomic way, without copying any bytes,
/// thus zero-copy deserialization.  Building off the ability to get a mutable reference, we can
/// also expose way to trivially update the underlying bytes through a safe interface, which can
/// avoid constantly serializing the entire type after changing a single field.
///
/// [archive]: rkyv::Archive
#[derive(Debug)]
pub struct BackedArchive<B, T> {
    backing: B,
    archive_len: usize,
    _archive: PhantomData<T>,
}

impl<B, T> BackedArchive<B, T>
where
    B: AsRef<[u8]>,
    T: Archive,
{
    /// Deserializes the archived value from the backing store and wraps it.
    ///
    /// # Errors
    ///
    /// If the data in the backing store is not valid for `T`, an error variant will be returned.
    pub fn from_backing(backing: B) -> Result<BackedArchive<B, T>, DeserializeError>
    where
        for<'a> T::Archived: CheckBytes<DefaultValidator<'a>>,
    {
        let archive_len = backing.as_ref().len();

        // Validate that the input is, well, valid.
        _ = check_archived_root::<T>(backing.as_ref())?;

        // Now that we know the buffer fits T, we're good to go!
        Ok(Self {
            backing,
            archive_len,
            _archive: PhantomData,
        })
    }

    /// Gets a reference to the backing store.
    pub fn get_backing_ref(&self) -> &B {
        &self.backing
    }

    /// Gets a reference to the archived value.
    pub fn get_archive_ref(&self) -> &T::Archived {
        let archive = &self.backing.as_ref()[..self.archive_len];
        // SAFETY: Both constructors validate this exact slice as an archived `T`.
        unsafe { archived_root::<T>(archive) }
    }
}

impl<B, T> BackedArchive<B, T>
where
    B: AsMut<[u8]>,
    T: Archive,
{
    /// Serializes the provided value to the backing store and wraps it.
    ///
    /// # Errors
    ///
    /// If there is an error during serializing of the value, an error variant will be returned that
    /// describes the error.  If the backing store is too small to hold the serialized version of
    /// the value, an error variant will be returned defining the minimum size the backing store
    /// must be, as well containing the value that failed to get serialized.
    pub fn from_value(mut backing: B, value: T) -> Result<BackedArchive<B, T>, SerializeError<T>>
    where
        T: Serialize<DefaultSerializer>,
        for<'a> T::Archived: CheckBytes<DefaultValidator<'a>>,
    {
        // Serialize our value so we can shove it into the backing.
        let mut serializer = DefaultSerializer::default();
        _ = serializer
            .serialize_value(&value)
            .map_err(|e| SerializeError::FailedToSerialize(e.to_string()))?;

        let src_buf = serializer.into_serializer().into_inner();

        // Now we have to write the serialized version to the backing store.  For obvious reasons,
        // the backing store needs to be able to hold the entire serialized representation, so we
        // check for that.  As well, instead of using `archived_root_mut`, we use
        // `archived_value_mut`, because this lets us relax need the backing store to be sized
        // _identically_ to the serialized size.
        let archive_len = src_buf.len();
        let dst_buf = backing.as_mut();
        if dst_buf.len() < archive_len {
            return Err(SerializeError::BackingStoreTooSmall(value, archive_len));
        }

        let archive = &mut dst_buf[..archive_len];
        archive.copy_from_slice(&src_buf);
        _ = check_archived_root::<T>(archive)
            .map_err(|error| SerializeError::FailedToSerialize(error.to_string()))?;

        Ok(Self {
            backing,
            archive_len,
            _archive: PhantomData,
        })
    }

    /// Gets a reference to the archived value.
    #[cfg(test)]
    pub fn get_archive_mut(&mut self) -> Pin<&mut T::Archived> {
        use rkyv::archived_root_mut;

        let pinned = Pin::new(&mut self.backing.as_mut()[..self.archive_len]);
        // SAFETY: Both constructors validate this exact slice, and `&mut self` guarantees
        // exclusive access for the lifetime of the returned projection.
        unsafe { archived_root_mut::<T>(pinned) }
    }
}

#[cfg(test)]
mod tests {
    use rkyv::AlignedVec;

    use super::{BackedArchive, SerializeError};

    #[test]
    fn from_value_preserves_archive_length_in_oversized_backing() {
        let mut backing = AlignedVec::new();
        backing.resize(16, 0);

        let archive = BackedArchive::from_value(backing, 42_u64).unwrap();

        assert_eq!(*archive.get_archive_ref(), 42);
    }

    #[test]
    fn from_value_rejects_unaligned_backing() {
        let mut backing = AlignedVec::new();
        backing.resize(9, 0);

        let result = BackedArchive::from_value(&mut backing[1..], 42_u64);

        assert!(matches!(result, Err(SerializeError::FailedToSerialize(_))));
    }
}
