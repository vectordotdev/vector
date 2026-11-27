//! Benchmark for `OctetCountingDecoder` framing (the syslog source framer).
//!
//! Two corpora:
//! - `newline_delimited`: RFC5424-style lines starting with `<` (non-transparent
//!   framing, the production profile for collector-syslog). Lines never start
//!   with an ASCII digit so the octet-counting auto-detection never triggers.
//! - `octet_counting`: RFC6587 3.4.1 transparent framing, `<len> <msg>` frames.

use std::{fmt, time::Duration};

use bytes::{BufMut, BytesMut};
use codecs::decoding::Framer;
use codecs::{Decoder, OctetCountingDecoder};
use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group,
    measurement::WallTime,
};
use tokio_util::codec::Decoder as _;

#[derive(Debug)]
struct Param {
    slug: &'static str,
    input: BytesMut,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug)
    }
}

const LINES: usize = 2000;

fn syslog_line(i: usize) -> Vec<u8> {
    let body = format!("bench message payload {i} {}", "y".repeat(200));
    format!("<34>1 2026-11-02T10:00:00.000Z mc-mt.ads memcached {i} - - {body}\n").into_bytes()
}

fn newline_corpus() -> BytesMut {
    let mut buf = BytesMut::new();
    for i in 0..LINES {
        buf.put_slice(&syslog_line(i));
    }
    buf
}

fn octet_corpus() -> BytesMut {
    let mut buf = BytesMut::new();
    for i in 0..LINES {
        let mut line = syslog_line(i);
        line.pop(); // drop trailing \n: octet counting frames are length-prefixed
        buf.put_slice(format!("{} ", line.len()).as_bytes());
        buf.put_slice(&line);
    }
    buf
}

fn decoding(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::codecs::decoding::Decoder::octet_counting");
    group.sampling_mode(SamplingMode::Auto);

    for param in &[
        Param {
            slug: "newline_delimited",
            input: newline_corpus(),
        },
        Param {
            slug: "octet_counting",
            input: octet_corpus(),
        },
    ] {
        group.throughput(Throughput::Bytes(param.input.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("octet_counting_bytes", param),
            &param,
            |b, param| {
                b.iter_batched(
                    || {
                        let framer = Framer::OctetCounting(OctetCountingDecoder::new());
                        let deserializer =
                            codecs::decoding::Deserializer::Bytes(codecs::BytesDeserializer);
                        let decoder = Decoder::new(framer, deserializer);
                        (Box::new(decoder), param.input.clone())
                    },
                    |(mut decoder, mut input)| loop {
                        match decoder.decode_eof(&mut input) {
                            Ok(Some(_)) => {}
                            Ok(None) => break,
                            Err(_) => unreachable!(),
                        }
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(5))
        .measurement_time(Duration::from_mins(1))
        .noise_threshold(0.01)
        .significance_level(0.05)
        .confidence_level(0.95)
        .nresamples(100_000)
        .sample_size(100);
    targets = decoding
);
