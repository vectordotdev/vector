use std::{fmt, io::Cursor};

use bytes::BytesMut;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use file_source::buffer::read_until_with_max_size;

struct Parameters {
    bytes: Vec<u8>,
    delim_offsets: Vec<usize>,
    delim: u8,
    bytes_before_first_delim: usize,
    max_size: u8,
}

impl fmt::Display for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "bytes_before_first_delim: {}",
            self.bytes_before_first_delim,
        )
    }
}

fn read_until_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("file-source");

    let mut parameters = vec![
        Parameters {
            bytes: vec![0; 1024],
            delim_offsets: vec![100, 500, 502],
            delim: 1,
            bytes_before_first_delim: 501,
            max_size: 1,
        },
        Parameters {
            bytes: vec![0; 1024],
            delim_offsets: vec![900, 999, 1004, 1021, 1023],
            delim: 1,
            bytes_before_first_delim: 1022,
            max_size: 1,
        },
    ];

    for param in &mut parameters {
        for offset in &param.delim_offsets {
            param.bytes[*offset] = param.delim;
        }
    }

    for param in &parameters {
        group.throughput(Throughput::Bytes(param.bytes_before_first_delim as u64));

        let mut position = 0;
        let mut buffer = BytesMut::with_capacity(param.max_size as usize);
        let mut reader = Cursor::new(&param.bytes);
        let delimiter: [u8; 1] = [param.delim];
        group.bench_with_input(BenchmarkId::new("read_until", param), &param, |b, _| {
            b.iter(|| {
                _ = read_until_with_max_size(
                    &mut reader,
                    &mut position,
                    &delimiter,
                    &mut buffer,
                    param.max_size as usize,
                );
                reader.set_position(0);
            })
        });
    }
}

criterion_group!(name = benches;
                 config = Criterion::default();
                 targets = read_until_bench);
criterion_main!(benches);
