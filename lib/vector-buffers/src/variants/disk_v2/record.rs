use std::mem;

use crc32fast::Hasher;
use rkyv::{
    Archive, Serialize,
};

use super::{
    common::align16,
    ser::{DeserializeError, try_as_archive},
};

pub const RECORD_HEADER_LEN: usize = align16(mem::size_of::<ArchivedRecord>() + 8);

/// Result of checking if a buffer contained a valid record.
pub enum RecordStatus {
    /// The record was able to be read from the buffer, and the checksum is valid.
    ///
    /// Contains the ID for the given record, as well as the metadata.
    Valid { id: u64 },
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
#[rkyv(attr(derive(Debug)))]
pub struct Record {
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
    payload: Vec<u8>,
}

impl Record {
    /// Creates a [`Record`] from the ID and payload, and calculates the checksum.
    pub fn with_checksum(id: u64, metadata: u32, payload: &[u8], checksummer: &Hasher) -> Self {
        let checksum = generate_checksum(checksummer, id, metadata, payload);
        Self {
            checksum,
            id,
            metadata,
            payload: payload.to_vec(),
        }
    }
}

impl ArchivedRecord {
    /// Gets the metadata of this record.
    pub fn metadata(&self) -> u32 {
        self.metadata.into()
    }

    /// Gets the payload of this record.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Verifies if the stored checksum of this record matches the record itself.
    pub fn verify_checksum(&self, checksummer: &Hasher) -> RecordStatus {
        let calculated = generate_checksum(checksummer, self.id.into(), self.metadata.into(), &self.payload);
        let checksum: u32 = self.checksum.into();
        let id: u64 = self.id.into();
        if checksum == calculated {
            RecordStatus::Valid { id }
        } else {
            RecordStatus::Corrupted {
                calculated,
                actual: checksum,
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
/// deserialization error encountered will be provided, which describes the error in a more verbose,
/// debugging-oriented fashion.
#[cfg_attr(test, instrument(skip_all, level = "trace"))]
pub fn validate_record_archive(buf: &[u8], checksummer: &Hasher) -> RecordStatus {
    match try_as_record_archive(buf) {
        Ok(archive) => archive.verify_checksum(checksummer),
        Err(e) => RecordStatus::FailedDeserialization(e),
    }
}

/// Attempts to deserialize an archived record from the given buffer.
///
/// The record archive is assumed to have been serialized as the very last item in `buf`, and
/// it is also assumed that the provided `buf` has an alignment of 16 bytes.
///
/// If a record archive was able to be read from the buffer, then a reference to its archived form
/// will be returned.  Otherwise, the deserialization error encountered will be provided, which describes the error in a more verbose,
/// debugging-oriented fashion.
#[cfg_attr(test, instrument(skip_all, level = "trace"))]
pub fn try_as_record_archive(buf: &[u8]) -> Result<&ArchivedRecord, DeserializeError> {
    try_as_archive::<Record>(buf)
}
