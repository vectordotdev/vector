use base64::{prelude::BASE64_STANDARD, Engine};
use bytes::Bytes;
use flate2::read::ZlibDecoder;

use vector::test_util::trace_init;

mod series;
mod sketches;

use super::*;

fn decompress_payload(payload: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let mut decompressor = ZlibDecoder::new(&payload[..]);
    let mut decompressed = Vec::new();
    let result = std::io::copy(&mut decompressor, &mut decompressed);
    result.map(|_| decompressed)
}

fn unpack_proto_payloads<T>(in_payloads: &FakeIntakeResponseRaw) -> Vec<T>
where
    T: prost::Message + std::default::Default,
{
    let mut out_payloads = vec![];

    in_payloads.payloads.iter().for_each(|payload| {
        // decode base64
        let payload = BASE64_STANDARD
            .decode(&payload.data)
            .expect("Invalid base64 data");

        // decompress
        let bytes = Bytes::from(decompress_payload(payload).unwrap());

        let payload = T::decode(bytes).unwrap();

        out_payloads.push(payload);
    });

    out_payloads
}

#[tokio::test]
async fn validate() {
    trace_init();

    series::validate().await;

    sketches::validate().await;
}
