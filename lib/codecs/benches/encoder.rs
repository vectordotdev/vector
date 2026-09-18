use bytes::{BufMut, BytesMut};
use codecs::{JsonSerializerConfig, NewlineDelimitedEncoder, encoding::Framer};
use criterion::{
    BatchSize, BenchmarkGroup, Criterion, Throughput, criterion_group, measurement::WallTime,
};
use tokio_util::codec::Encoder;
use vector_common::{Error, btreemap, byte_size_of::ByteSizeOf};
use vector_core::event::{Event, LogEvent};

#[derive(Debug, Clone)]
pub struct JsonLogSerializer;

impl Encoder<Event> for JsonLogSerializer {
    type Error = Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let writer = buffer.writer();
        let log = event.as_log();
        serde_json::to_writer(writer, log)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JsonLogVecSerializer;

impl Encoder<Event> for JsonLogVecSerializer {
    type Error = Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.as_log();
        let vec = serde_json::to_vec(log)?;
        buffer.put_slice(&vec);
        Ok(())
    }
}

fn encoder(c: &mut Criterion) {
    let mut group: BenchmarkGroup<WallTime> = c.benchmark_group("encoder");

    let input: Event = Event::Log(LogEvent::from(btreemap! {
        "key1" => "value1",
        "key2" => "value2",
        "key3" => "value3"
    }));

    group.throughput(Throughput::Bytes(input.size_of() as u64));
    group.bench_with_input("JsonLogVecSerializer::encode", &(), |b, ()| {
        b.iter_batched(
            || JsonLogVecSerializer,
            |mut encoder| {
                let mut bytes = BytesMut::new();
                encoder.encode(input.clone(), &mut bytes).unwrap();
                bytes.put_u8(b'\n');
            },
            BatchSize::SmallInput,
        )
    });

    group.throughput(Throughput::Bytes(input.size_of() as u64));
    group.bench_with_input("JsonLogSerializer::encode", &(), |b, ()| {
        b.iter_batched(
            || JsonLogSerializer,
            |mut encoder| {
                let mut bytes = BytesMut::new();
                encoder.encode(input.clone(), &mut bytes).unwrap();
                bytes.put_u8(b'\n');
            },
            BatchSize::SmallInput,
        )
    });

    group.throughput(Throughput::Bytes(input.size_of() as u64));
    group.bench_with_input("codecs::JsonSerializer::encode", &(), |b, ()| {
        b.iter_batched(
            || JsonSerializerConfig::default().build(),
            |mut encoder| {
                let mut bytes = BytesMut::new();
                encoder.encode(input.clone(), &mut bytes).unwrap();
                bytes.put_u8(b'\n');
            },
            BatchSize::SmallInput,
        )
    });

    group.throughput(Throughput::Bytes(input.size_of() as u64));
    group.bench_with_input("vector::codecs::Encoder::encode", &(), |b, ()| {
        b.iter_batched(
            || {
                codecs::Encoder::<Framer>::new(
                    NewlineDelimitedEncoder::default().into(),
                    JsonSerializerConfig::default().build().into(),
                )
            },
            |mut encoder| {
                let mut bytes = BytesMut::new();
                encoder.encode(input.clone(), &mut bytes).unwrap();
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, encoder);
