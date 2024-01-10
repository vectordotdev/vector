use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use data_encoding::BASE64;
use dnsmsg_parser::dns_message_parser::DnsMessageParser;
use hickory_proto::rr::rdata::NULL;

fn benchmark_parse_as_query_message(c: &mut Criterion) {
    let raw_dns_message = "szgAAAABAAAAAAAAAmg1B2V4YW1wbGUDY29tAAAGAAE=";
    let raw_query_message = BASE64.decode(raw_dns_message.as_bytes()).unwrap();

    let mut group = c.benchmark_group("dnstap");
    group.throughput(Throughput::Bytes(raw_query_message.len() as u64));
    group.bench_function("parse_as_query_message", |b| {
        b.iter_batched(
            || DnsMessageParser::new(raw_query_message.clone()),
            |mut parser| parser.parse_as_query_message().unwrap(),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn benchmark_parse_as_update_message(c: &mut Criterion) {
    let raw_dns_message = "xjUoAAABAAAAAQAAB2V4YW1wbGUDY29tAAAGAAECaDXADAD/AP8AAAAAAAA=";
    let raw_update_message = BASE64.decode(raw_dns_message.as_bytes()).unwrap();

    let mut group = c.benchmark_group("dnstap");
    group.throughput(Throughput::Bytes(raw_update_message.len() as u64));
    group.bench_function("parse_as_update_message", |b| {
        b.iter_batched(
            || DnsMessageParser::new(raw_update_message.clone()),
            |mut parser| parser.parse_as_update_message().unwrap(),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn benchmark_parse_wks_rdata(c: &mut Criterion) {
    benchmark_parse_rdata(c, "gAgBDgYAAAFA", 11, "parse_wks_rdata");
}

fn benchmark_parse_a6_rdata(c: &mut Criterion) {
    benchmark_parse_rdata(
        c,
        "QBI0VniavN7wCFNVQk5FVC0xA0lQNghleGFtcGxlMQNjb20A",
        38,
        "parse_a6_rdata",
    );
}

fn benchmark_parse_loc_rdata(c: &mut Criterion) {
    benchmark_parse_rdata(c, "ADMWE4kXLdBwvhXwAJiNIA==", 29, "parse_loc_rdata");
}

fn benchmark_parse_apl_rdata(c: &mut Criterion) {
    benchmark_parse_rdata(c, "AAEVA8CoIAABHIPAqCY=", 42, "parse_apl_rdata");
}

fn benchmark_parse_rdata(c: &mut Criterion, data: &str, code: u16, id: &str) {
    let raw_rdata = BASE64.decode(data.as_bytes()).unwrap();

    let record_rdata = NULL::with(raw_rdata.clone());

    let mut group = c.benchmark_group("dnstap");
    group.throughput(Throughput::Bytes(raw_rdata.len() as u64));
    group.bench_function(id, |b| {
        b.iter_batched(
            || {
                (
                    record_rdata.clone(),
                    DnsMessageParser::new(Vec::<u8>::new()),
                )
            },
            |(record_rdata, mut parser)| parser.format_unknown_rdata(code, &record_rdata).unwrap(),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/vectordotdev/vector/pull/6408
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_parse_as_query_message,
              benchmark_parse_as_update_message,
              benchmark_parse_wks_rdata,
              benchmark_parse_a6_rdata,
              benchmark_parse_loc_rdata,
              benchmark_parse_apl_rdata,
);
criterion_main!(benches);
