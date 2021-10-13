use std::io::{self, Write};

use vector_core::event::Metric;

use crate::sinks::util::encoding::StatefulEncoder;
use crate::sinks::util::{Compression, Compressor};

use super::config::{DatadogMetricsEndpoint, MAXIMUM_SERIES_PAYLOAD_SIZE, MAXIMUM_SERIES_PAYLOAD_COMPRESSED_SIZE};

const SERIES_PAYLOAD_HEADER: &[u8] = b"{\"series\":[";
const SERIES_PAYLOAD_FOOTER: &[u8] = b"]}";

pub struct DatadogMetricsEncoder {
    endpoint: DatadogMetricsEndpoint,
    writer: Compressor,
    written: usize,
    buf: Vec<u8>,
    uncompressed_limit: Option<usize>,
    compressed_limit: Option<usize>,
}

impl DatadogMetricsEncoder {
    /// Creates a new `DatadogMetricsEncoder` for the given endpoint.
    /// 
    /// Depending on the endpoint, different payload size limitations will be applied.
    pub fn new(endpoint: DatadogMetricsEndpoint, compression: Compression) -> io::Result<Self> {
        // Calculate the payload size limits for the given endpoint.
        let (uncompressed_limit, mut compressed_limit) = get_endpoint_payload_size_limits(endpoint);
        if !compression.is_compressed() {
            // No reason to have a compressed limit if we aren't actually compressing.
            let _ = compressed_limit.take();
        }

        // Create our compressor and get the header in place.
        let mut writer = Compressor::from(compression);
        let _ = write_payload_header(endpoint, &mut writer)?;

        Ok(Self {
            endpoint,
            writer,
            written: 0,
            buf: Vec::new(),
            uncompressed_limit,
            compressed_limit,
        })
    }
}

impl StatefulEncoder<Metric> for DatadogMetricsEncoder {
    type Payload = Vec<u8>;

    fn try_encode(&mut self, event: Metric) -> io::Result<Option<Metric>> {
        // Start out by encoding the metric into our temporary buffer.
        //
        // I also realize that we're grabbing the length of the temporary buffer even though we got
        // back the number of bytes written by the actual encode function, but this is purely to
        // ensure that the number we're considering is the true buffer length, rather than any
        // intermediate number that may have unintentionally been passed back.
        self.buf.clear();
        let n = encode_metric_for_endpoint(self.endpoint, &event, &mut self.buf)
            .map(|_| self.buf.len())?;

        // If we're over our uncompressed size limit with this metric, inform the caller.
        if let Some(uncompressed_limit) = self.uncompressed_limit {
            if self.written + n > uncompressed_limit {
                return Ok(Some(event))
            }
        }

        // Calculating the compressed size is slightly more tricky, because we can only speculate
        // about how many bytes it would take when compressed.  If we write into the compressor, we
        // can't back out that write, even if we manually modify the underlying Vec<u8>, as the
        // compressor might have internal state around checksums, etc, that can't be similarly
        // rolled back.
        //
        // Our strategy is thus: simply take the encoded-but-decompressed size and see if it would
        // fit within the compressed limit.  The worst case scenario should be when the input is
        // incompressible, so while not optimal, this also ensures we wouldn't exceed the compressed
        // limit.
        //
        // TODO: Could we track the ratio between uncompressed/compressed metrics as we write
        // them, in order to be able to estimate the size of a metric once written to the
        // compressor? This wuld mean that we would potentially ruin an entire batch if we wrote to the
        // compressor and our estimate was too low, which would make the type signature ugly and
        // also mean that the caller would have to track that fact to avoid hitting it again.
        //
        // Might be more easily achieved if we could write to the compressor knowing that it would
        // calculate the CRC at the very end, giving us a chance to back out a compressed write if
        // it would indeed overflow, but flate2 does not currently have a way to let us do that.
        if let Some(compressed_limit) = self.compressed_limit {
            let compressed_len = self.writer.get_ref().len();
            if compressed_len + n > compressed_limit {
                return Ok(Some(event))
            }
        }

        // We should be safe to write our holding buffer to the compressor.
        let _ = self.writer.write_all(&self.buf)?;
        self.written += n;

        Ok(None)
    }

    fn finish(mut self) -> io::Result<Option<Vec<u8>>> {
        let _ = write_payload_footer(self.endpoint, &mut self.writer)?;
        let payload = self.writer.finish()?;

        // A compressed limit is only set if we're actually compressing, so we check for that, and
        // then the uncompressed size, and if neither are set, we default to returning the payload.
        let within_limit = self.compressed_limit.or(self.uncompressed_limit)
            .map(|limit| payload.len() <= limit)
            .unwrap_or(true);
        
        Ok(within_limit.then(|| payload))
    }
}

fn encode_metric_for_endpoint(_endpoint: DatadogMetricsEndpoint, _metric: &Metric, _buf: &mut dyn io::Write) -> io::Result<usize> {
    todo!()
}

fn get_endpoint_payload_size_limits(endpoint: DatadogMetricsEndpoint) -> (Option<usize>, Option<usize>) {
    // Estimate the potential overhead of the compression container itself.
    //
    // TODO: We're estimating the expected size of the compressor overhead -- file header,
    // checksum, etc -- and computing our actual compressed limit from that.  This should be
    // reasonably accurate for gzip, but might not be for other compression algorithms.
    //
    // flate2 does not expose a way for us to get the exact numbers, but it would be nice if
    // it did.
    let estimated_compressed_header_len = 24;

    match endpoint {
        DatadogMetricsEndpoint::Series => {
            let uncompressed_limit = MAXIMUM_SERIES_PAYLOAD_SIZE;
            let compressed_limit = MAXIMUM_SERIES_PAYLOAD_COMPRESSED_SIZE;

            // We already know we'll have to write the header/footer for the series payload by hand
            // to allow encoding incrementally, so figure out the size of that so we can remove it.
            let header_len = SERIES_PAYLOAD_HEADER.len() + SERIES_PAYLOAD_FOOTER.len();

            // This is a sanity check to ensure that our chosen limits are reasonable.
            assert!(uncompressed_limit > header_len);
            assert!(compressed_limit > header_len + estimated_compressed_header_len);

            // Adjust for the known/estimated sizes of headers, footers, compression container
            // overhead, etc.
            let uncompressed_limit = uncompressed_limit - header_len;
            let compressed_limit = compressed_limit - header_len + estimated_compressed_header_len;

            (Some(uncompressed_limit), Some(compressed_limit))
        },
        // TODO: figure out reasonable/actual payload size limits for distributions and sketches.
        _ => (None, None)
    }
}

fn write_payload_header(endpoint: DatadogMetricsEndpoint, writer: &mut dyn io::Write) -> io::Result<()> {
    match endpoint {
        DatadogMetricsEndpoint::Series => writer.write_all(SERIES_PAYLOAD_HEADER),
        _ => Ok(()),
    }
}

fn write_payload_footer(endpoint: DatadogMetricsEndpoint, writer: &mut dyn io::Write) -> io::Result<()> {
    match endpoint {
        DatadogMetricsEndpoint::Series => writer.write_all(SERIES_PAYLOAD_FOOTER),
        _ => Ok(()),
    }
}