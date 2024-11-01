use std::{fmt, time::Duration};

use bytes::BytesMut;
use criterion::{
    criterion_group, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId, Criterion,
    SamplingMode, Throughput,
};
use tokio_util::codec::Decoder;
use vector_lib::codecs::{
    decoding::Deserializer, decoding::Framer, BytesDeserializer, NewlineDelimitedDecoder,
};

#[derive(Debug)]
struct Param {
    slug: &'static str,
    input: BytesMut,
    max_length: Option<usize>,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug)
    }
}

fn decoding(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector::codecs::decoding::Decoder");
    group.sampling_mode(SamplingMode::Auto);

    for param in &[
        Param {
            slug: "no_max",
            input: BytesMut::from(include_str!("moby_dick.txt")),
            max_length: None,
        },
        Param {
            slug: "small_max",
            input: BytesMut::from(include_str!("moby_dick.txt")),
            max_length: Some(10),
        },
    ] {
        group.throughput(Throughput::Bytes(param.input.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("newline_bytes", param),
            &param,
            |b, param| {
                b.iter_batched(
                    || {
                        let framer = Framer::NewlineDelimited(
                            param
                                .max_length
                                .map(|ml| NewlineDelimitedDecoder::new_with_max_length(ml))
                                .unwrap_or(NewlineDelimitedDecoder::new()),
                        );
                        let deserializer = Deserializer::Bytes(BytesDeserializer);
                        let decoder = vector::codecs::Decoder::new(framer, deserializer);

                        (Box::new(decoder), param.input.clone())
                    },
                    |(mut decoder, mut input)| loop {
                        match decoder.decode_eof(&mut input) {
                            Ok(Some(_)) => continue,
                            Ok(None) => break,
                            Err(_) => {
                                unreachable!()
                            }
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(5))
        .measurement_time(Duration::from_secs(120))
        // degree of noise to ignore in measurements, here 1%
        .noise_threshold(0.01)
        // likelihood of noise registering as difference, here 5%
        .significance_level(0.05)
        // likelihood of capturing the true runtime, here 95%
        .confidence_level(0.95)
        // total number of bootstrap resamples, higher is less noisy but slower
        .nresamples(100_000)
        // total samples to collect within the set measurement time
        .sample_size(150);
    targets = decoding
);
