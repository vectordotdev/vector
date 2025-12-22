use std::io::Read;

use async_compression::tokio::bufread::{ZstdDecoder, ZstdEncoder};
use base64::{Engine, prelude::BASE64_STANDARD};
use bytes::Bytes;
use flate2::read::ZlibDecoder;
use serde_json::Value;
use tokio::io::{AsyncReadExt, BufReader};
use tracing::{debug, warn};
use vector::test_util::{compression::is_zstd, trace_init};
use vector_common::Result;

mod series;
mod sketches;

use super::*;

async fn decompress_payload(payload: &[u8]) -> std::io::Result<Vec<u8>> {
    if is_zstd(payload) {
        let mut decompressor = ZstdDecoder::new(payload);
        let mut decompressed = Vec::new();
        decompressor.read_to_end(&mut decompressed).await?;
        debug!(
            "Zstd decompression successful: {} -> {} bytes",
            payload.len(),
            decompressed.len()
        );
        return Ok(decompressed);
    }

    let mut decompressor = ZlibDecoder::new(payload);
    let mut decompressed = Vec::new();
    let result = decompressor.read_to_end(&mut decompressed);
    if let Ok(size) = &result {
        debug!(
            "Zlib decompression successful: {} -> {} bytes",
            payload.len(),
            size
        );
    }
    result.map(|_| decompressed)
}

async fn unpack_proto_payloads<T>(in_payloads: &FakeIntakeResponseRaw) -> Result<Vec<T>>
where
    T: prost::Message + Default,
{
    let mut out_payloads = vec![];

    for payload in &in_payloads.payloads {
        // decode base64
        let payload = BASE64_STANDARD
            .decode(&payload.data)
            .map_err(|e| format!("Invalid base64 data: {}", e))?;

        // Skip empty or near-empty payloads (e.g., health checks like '{}' sent with
        // X-Requested-With: datadog-agent-diagnose header)
        if payload.len() < 10 {
            let content_str = String::from_utf8_lossy(&payload);

            // Try to parse as JSON to show structured content
            let json_repr = serde_json::from_slice::<Value>(&payload)
                .map(|v| format!("JSON: {}", v))
                .unwrap_or_else(|_| format!("raw: '{}'", content_str));

            warn!(
                "Skipping small payload (likely diagnostic/health check): expected protobuf type {}, got {} bytes, content: {}, hex: {:02x?}",
                std::any::type_name::<T>(),
                payload.len(),
                json_repr,
                payload
            );
            continue;
        }

        // Try to decode directly first (handles decompressed data from fakeintake)
        let decoded = match T::decode(Bytes::from(payload.clone())) {
            Ok(decoded) => {
                // Successfully decoded directly - fakeintake returned decompressed data
                debug!(
                    "Decoded protobuf directly (fakeintake returned decompressed): type {}, size {} bytes",
                    std::any::type_name::<T>(),
                    payload.len()
                );
                decoded
            }
            Err(_) => {
                // Direct decode failed - payload is still compressed, decompress first
                debug!(
                    "Direct decode failed, attempting decompression: type {}, size {} bytes",
                    std::any::type_name::<T>(),
                    payload.len()
                );
                let decompressed = decompress_payload(payload.as_slice())
                    .await
                    .map_err(|e| format!(
                        "Failed to decompress payload: {}. Type {}, length {}, first 4 bytes: {:02x?}",
                        e,
                        std::any::type_name::<T>(),
                        payload.len(),
                        &payload[..payload.len().min(4)]
                    ))?;
                T::decode(Bytes::from(decompressed)).map_err(|e| {
                    format!(
                        "Failed to decode protobuf after decompression: {} (type {})",
                        e,
                        std::any::type_name::<T>()
                    )
                })?
            }
        };

        out_payloads.push(decoded);
    }

    Ok(out_payloads)
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
