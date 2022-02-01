use std::ptr::addr_of;

use bytecheck::{CheckBytes, ErrorBox, StructCheckError};
use crc32fast::Hasher;
use rkyv::{
    boxed::ArchivedBox,
    with::{CopyOptimize, RefAsBox},
    Archive, Archived, Serialize,
};

use super::ser::{try_as_archive, DeserializeError};

/// Result of checking if a buffer contained a valid record.
pub enum RecordStatus {
    /// The record was able to be read from the buffer, and the checksum is valid.
    ///
    /// Contains the ID for the given record, as well as the metadata.
    Valid { id: u64, metadata: u32 },
    /// The record was able to be read from the buffer, but the checksum was not valid.
    Corrupted { calculated: u32, actual: u32 },
    /// The record was not able to be read from the buffer due to an error during deserialization.
    FailedDeserialization(DeserializeError),
}

/// Record container.
///
/// [`Record`] encapsulates the encoded form of a record written into the buffer.  It is a simple wrapper that
/// carries only the necessary metadata: the record checksum, and a record ID used internally for
/// properly tracking the state of the reader and writer.
///
/// # Warning
///
/// - Do not add fields to this struct.
/// - Do not remove fields from this struct.
/// - Do not change the type of fields in this struct.
/// - Do not change the order of fields this struct.
///
/// Doing so will change the serialized representation.  This will break things.
///
/// Do not do any of the listed things unless you _absolutely_ know what you're doing. :)
#[derive(Archive, Serialize, Debug)]
// Switch back to the derived implementation of CheckBytes once the upstream ICE issue is fixed.
//
// Upstream issue: https://github.com/rkyv/rkyv/issues/221
//#[archive_attr(derive(CheckBytes))]
pub struct Record<'a> {
    /// The checksum of the record.
    ///
    /// The checksum is CRC32C(BE(id) + BE(metadata) + payload), where BE(x) returns a byte slice of
    /// the given integer in big endian format.
    pub(super) checksum: u32,

    /// The record ID.
    ///
    /// This is monotonic across records.
    id: u64,

    /// The record metadata.
    ///
    /// Based on `Encodable::Metadata`.
    pub(super) metadata: u32,

    /// The record payload.
    ///
    /// This is the encoded form of the actual record itself.
    #[with(CopyOptimize, RefAsBox)]
    payload: &'a [u8],
}

// Manual implementation of CheckBytes required as the derived version currently causes an internal
// compiler error.
//
// Upstream issue: https://github.com/rkyv/rkyv/issues/221
impl<'a, C: ?Sized> CheckBytes<C> for ArchivedRecord<'a>
where
    rkyv::with::With<&'a [u8], RefAsBox>: Archive<Archived = ArchivedBox<[u8]>>,
    ArchivedBox<[u8]>: CheckBytes<C>,
{
    type Error = StructCheckError;
    unsafe fn check_bytes<'b>(
        value: *const Self,
        context: &mut C,
    ) -> Result<&'b Self, Self::Error> {
        Archived::<u32>::check_bytes(addr_of!((*value).checksum), context).map_err(|e| {
            StructCheckError {
                field_name: "checksum",
                inner: ErrorBox::new(e),
            }
        })?;
        Archived::<u64>::check_bytes(addr_of!((*value).id), context).map_err(|e| {
            StructCheckError {
                field_name: "id",
                inner: ErrorBox::new(e),
            }
        })?;
        Archived::<u32>::check_bytes(addr_of!((*value).metadata), context).map_err(|e| {
            StructCheckError {
                field_name: "schema_metadata",
                inner: ErrorBox::new(e),
            }
        })?;
        ArchivedBox::<[u8]>::check_bytes(addr_of!((*value).payload), context).map_err(|e| {
            StructCheckError {
                field_name: "payload",
                inner: ErrorBox::new(e),
            }
        })?;
        Ok(&*value)
    }
}

impl<'a> Record<'a> {
    /// Creates a [`Record`] from the ID and payload, and calculates the checksum.
    pub fn with_checksum(id: u64, metadata: u32, payload: &'a [u8], checksummer: &Hasher) -> Self {
        let checksum = generate_checksum(checksummer, id, metadata, payload);
        Self {
            checksum,
            id,
            metadata,
            payload,
        }
    }
}

impl<'a> ArchivedRecord<'a> {
    /// Gets the metadata of this record.
    pub fn metadata(&self) -> u32 {
        self.metadata
    }

    /// Gets the payload of this record.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Verifies if the stored checksum of this record matches the record itself.
    pub fn verify_checksum(&self, checksummer: &Hasher) -> RecordStatus {
        let calculated = generate_checksum(checksummer, self.id, self.metadata, &self.payload);
        if self.checksum == calculated {
            RecordStatus::Valid {
                id: self.id,
                metadata: self.metadata,
            }
        } else {
            RecordStatus::Corrupted {
                calculated,
                actual: self.checksum,
            }
        }
    }
}

fn generate_checksum(checksummer: &Hasher, id: u64, metadata: u32, payload: &[u8]) -> u32 {
    let mut checksummer = checksummer.clone();
    checksummer.reset();

    checksummer.update(&id.to_be_bytes()[..]);
    checksummer.update(&metadata.to_be_bytes()[..]);
    checksummer.update(payload);
    checksummer.finalize()
}

/// Checks whether the given buffer contains a valid [`Record`] archive.
///
/// The record archive is assumed to have been serialized as the very last item in `buf`, and
/// it is also assumed that the provided `buf` has an alignment of 8 bytes.
///
/// If a record archive was able to be read from the buffer, then the status will indicate whether
/// or not the checksum in the record matched the recalculated checksum.  Otherwise, the
/// deserialization error encounted will be provided, which describes the error in a more verbose,
/// debugging-oriented fashion.
#[cfg_attr(test, instrument(skip_all, level = "trace"))]
pub fn try_as_record_archive(buf: &[u8], checksummer: &Hasher) -> RecordStatus {
    match try_as_archive::<Record<'_>>(buf) {
        Ok(archive) => archive.verify_checksum(checksummer),
        Err(e) => RecordStatus::FailedDeserialization(e),
    }
}
