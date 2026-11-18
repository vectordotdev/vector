//! The official KPL proto2 wire format, with one partition key per aggregate.
//!
//! <https://github.com/awslabs/amazon-kinesis-producer/blob/master/aggregation-format.md>

use md5::{Digest, Md5};
use prost::encoding::{WireType, encode_key, encode_varint, encoded_len_varint};

const MAGIC: [u8; 4] = [0xf3, 0x89, 0x9a, 0xc2];

const fn user_record_len(payload_len: usize) -> usize {
    // Required partition_key_index=0 (two bytes), required data tag and length.
    3 + encoded_len_varint(payload_len as u64) + payload_len
}

pub(super) const fn record_len(payload_len: usize) -> usize {
    let len = user_record_len(payload_len);
    1 + encoded_len_varint(len as u64) + len
}

pub(super) const fn overhead(key_len: usize) -> usize {
    // Magic, MD5, and the single entry in partition_key_table (field 1).
    21 + encoded_len_varint(key_len as u64) + key_len
}

pub(super) fn append_record(payload: &[u8], output: &mut Vec<u8>) {
    encode_key(3, WireType::LengthDelimited, output);
    encode_varint(user_record_len(payload.len()) as u64, output);
    encode_key(1, WireType::Varint, output);
    encode_varint(0, output);
    encode_key(3, WireType::LengthDelimited, output);
    encode_varint(payload.len() as u64, output);
    output.extend_from_slice(payload);
}

pub(super) fn envelope(key: &str, records: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(overhead(key.len()) + records.len());
    output.extend_from_slice(&MAGIC);
    encode_key(1, WireType::LengthDelimited, &mut output);
    encode_varint(key.len() as u64, &mut output);
    output.extend_from_slice(key.as_bytes());
    output.extend_from_slice(records);
    let digest = Md5::digest(&output[MAGIC.len()..]);
    output.extend_from_slice(&digest);
    output
}

#[cfg(test)]
pub(super) mod test_support {
    use md5::{Digest, Md5};
    use prost::Message;

    // Decode using typed proto2 messages, independently of the wire encoder.
    #[derive(Clone, PartialEq, Message)]
    struct Record {
        #[prost(uint64, required, tag = "1")]
        partition_key_index: u64,
        #[prost(uint64, optional, tag = "2")]
        explicit_hash_key_index: Option<u64>,
        #[prost(bytes, required, tag = "3")]
        data: Vec<u8>,
    }

    #[derive(Clone, PartialEq, Message)]
    struct AggregatedRecord {
        #[prost(string, repeated, tag = "1")]
        partition_key_table: Vec<String>,
        #[prost(string, repeated, tag = "2")]
        explicit_hash_key_table: Vec<String>,
        #[prost(message, repeated, tag = "3")]
        records: Vec<Record>,
    }

    pub(crate) fn decode(payload: &[u8], outer_key: &str) -> Vec<Vec<u8>> {
        assert_eq!(&payload[..4], &[0xf3, 0x89, 0x9a, 0xc2]);
        let data = &payload[4..payload.len() - 16];
        assert_eq!(Md5::digest(data).as_slice(), &payload[payload.len() - 16..]);
        let decoded = AggregatedRecord::decode(data).unwrap();
        assert_eq!(decoded.explicit_hash_key_table.len(), 0);
        assert_eq!(decoded.partition_key_table, [outer_key]);
        decoded
            .records
            .into_iter()
            .map(|record| {
                assert_eq!(record.partition_key_index, 0);
                assert!(record.explicit_hash_key_index.is_none());
                record.data
            })
            .collect()
    }
}
