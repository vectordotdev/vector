use criterion::{criterion_group, BatchSize, Criterion, Throughput};
use rand::{rngs::SmallRng, thread_rng, Rng, SeedableRng};
use rand_distr::{Alphanumeric, Distribution, Uniform};
use vector::{
    config::{TransformConfig, TransformContext},
    event::Event,
    test_util::runtime,
    transforms,
};

fn benchmark_regex(c: &mut Criterion) {
    let lines: Vec<String> = http_access_log_lines().take(10).collect();

    let mut group = c.benchmark_group("regex");
    group.throughput(Throughput::Bytes(
        lines.iter().fold(0, |sum, l| sum + l.len()) as u64,
    ));

    let input: Vec<Event> = lines.into_iter().map(|l| l.into()).collect();

    group.bench_function("http", |b| {
        let rt = runtime();

        let mut parser = rt.block_on(async move {
            transforms::regex_parser::RegexParserConfig {
                // many captures to stress the regex parser
                patterns: vec![r#"^(?P<addr>\d+\.\d+\.\d+\.\d+) (?P<user>\S+) (?P<auth>\S+) \[(?P<date>\d+/[A-Za-z]+/\d+:\d+:\d+:\d+ [+-]\d{4})\] "(?P<method>[A-Z]+) (?P<uri>[^"]+) HTTP/\d\.\d" (?P<code>\d+) (?P<size>\d+) "(?P<referrer>[^"]+)" "(?P<browser>[^"]+)""#.into()],
                field: None,
                drop_failed: true,
                ..Default::default()
            }
            .build(&TransformContext::default())
            .await
            .unwrap().into_function()
        });

        b.iter_batched(
            || {
                (input.clone(), transforms::OutputBuffer::with_capacity(input.len()))
            },
            |(events, mut output)| {
                let event_count = events.len();

                events.into_iter().for_each(|event| parser.transform(&mut output, event));

                debug_assert_eq!(output.len(), event_count);

                output
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn http_access_log_lines() -> impl Iterator<Item = String> {
    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();
    let code = Uniform::from(200..600);
    let year = Uniform::from(2010..2020);
    let mday = Uniform::from(1..32);
    let hour = Uniform::from(0..24);
    let minsec = Uniform::from(0..60);
    let size = Uniform::from(10..60); // FIXME

    std::iter::repeat(()).map(move |_| {
        let url_size = size.sample(&mut rng);
        let browser_size = size.sample(&mut rng);
        format!("{}.{}.{}.{} - - [{}/Jun/{}:{}:{}:{} -0400] \"GET /{} HTTP/1.1\" {} {} \"-\" \"Mozilla/5.0 ({})\"",
                rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(), // IP
                year.sample(&mut rng), mday.sample(&mut rng), // date
                hour.sample(&mut rng), minsec.sample(&mut rng), minsec.sample(&mut rng), // time
                (&mut rng).sample_iter(&Alphanumeric).take(url_size).map(char::from).collect::<String>(), // URL
                code.sample(&mut rng), size.sample(&mut rng),
                (&mut rng).sample_iter(&Alphanumeric).take(browser_size).map(char::from).collect::<String>(),
        )
    })
}

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.07);
    targets = benchmark_regex
);
