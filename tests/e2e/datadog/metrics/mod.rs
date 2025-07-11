use async_compression::tokio::bufread::{ZstdDecoder, ZstdEncoder};
use base64::{prelude::BASE64_STANDARD, Engine};
use bytes::Bytes;
use flate2::read::ZlibDecoder;
use std::io::Read;
use tokio::io::{AsyncReadExt, BufReader};
use vector::test_util::compression::is_zstd;
use vector::test_util::trace_init;

mod series;
mod sketches;

use super::*;

async fn decompress_payload(payload: &[u8]) -> std::io::Result<Vec<u8>> {
    if is_zstd(&payload) {
        let mut decompressor = ZstdDecoder::new(payload);
        let mut decompressed = Vec::new();
        decompressor.read_to_end(&mut decompressed).await?;
        return Ok(decompressed);
    }

    let mut decompressor = ZlibDecoder::new(&payload[..]);
    let mut decompressed = Vec::new();
    let result = decompressor.read_to_end(&mut decompressed);
    result.map(|_| decompressed)
}

async fn unpack_proto_payloads<T>(in_payloads: &FakeIntakeResponseRaw) -> Vec<T>
where
    T: prost::Message + Default,
{
    let mut out_payloads = vec![];

    for payload in &in_payloads.payloads {
        // decode base64
        let payload = BASE64_STANDARD
            .decode(&payload.data)
            .expect("Invalid base64 data");

        // decompress
        let bytes = Bytes::from(decompress_payload(payload.as_slice()).await.unwrap());

        let payload = T::decode(bytes).unwrap();

        out_payloads.push(payload);
    }

    out_payloads
}

#[tokio::test]
async fn validate() {
    trace_init();

    // Even with configuring docker service dependencies, we need a small buffer of time
    // to ensure events flow through to fakeintake before asking for them
    std::thread::sleep(std::time::Duration::from_secs(2));

    series::validate().await;

    sketches::validate().await;
}

async fn compress_with_zstd(data: &[u8]) -> Vec<u8> {
    let reader = BufReader::new(data);
    let mut encoder = ZstdEncoder::new(reader);
    let mut compressed_data = Vec::new();
    encoder
        .read_to_end(&mut compressed_data)
        .await
        .expect("unexpected compression error");
    compressed_data
}

#[tokio::test]
async fn test_decompress_payload_zstd() {
    let original_data = b"Hello, Zstd!";
    let compressed_data = compress_with_zstd(original_data).await;

    let decompressed_data = decompress_payload(compressed_data.as_slice())
        .await
        .expect("decompression failed");
    assert_eq!(decompressed_data, original_data);
}
