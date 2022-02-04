use criterion::{black_box, criterion_group, criterion_main, Criterion};
use datadog_filter::{build_matcher, fast_matcher, Matcher, Run};
use serde_json::json;
use vector::event::{Event, LogEvent};
use vector_datadog_filter::EventFilter;

criterion_group!(
    name = benches;
    config = Criterion::default().noise_threshold(0.02);
    targets = benchmark_dd_search
);
criterion_main!(benches);

const PAT: &str = "source:(agent OR datadog-agent OR datadog-agent-cluster-worker OR datadog-cluster-agent OR process-agent OR security-agent OR system-probe OR trace-agent OR nginx)";

fn benchmark_dd_search(c: &mut Criterion) {
    let mut c = c.benchmark_group("filtering");
    c.bench_function("matcher", |b| {
        let node = datadog_search_syntax::parse(PAT).expect("bad syntax");
        let matcher = as_log(build_matcher(&node, &EventFilter::default()));
        let events = build_events();
        b.iter(|| {
            for event in &events {
                matcher.run(black_box(&event));
            }
        });
    });

    c.bench_function("fast_matcher", |b| {
        let node = datadog_search_syntax::parse(PAT).expect("bad syntax");
        let matcher = fast_matcher::build_matcher(&node, &EventFilter::default());
        let events = build_events();
        b.iter(|| {
            for event in &events {
                if let Event::Log(log) = &event {
                    EventFilter::run(&matcher, black_box(log));
                } else {
                    false;
                }
            }
        });
    });
    c.finish();
}

// copied from transform setup
fn as_log(matcher: Box<dyn Matcher<LogEvent>>) -> Box<dyn Matcher<Event>> {
    Run::boxed(move |ev| match ev {
        Event::Log(log) => matcher.run(log),
        _ => false,
    })
}

fn build_events() -> Vec<Event> {
    let raw = vec![
        json!({ "source" : "nginx", "message": "127.0.0.1 - frank [13/Jul/2016:10:55:36 +0000] \"GET /apache_pb.gif HTTP/1.0\" 200 2326" }),
        json!({ "source" : "apache", "message": "172.17.0.1 - - [06/Jan/2017:16:16:37 +0000] \"GET /datadoghq/company?test=var1%20Pl HTTP/1.1\" 404 612 \"http://www.perdu.com/\" \"Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36\" \"-\"" }),
        json!({ "source" : "nginx", "message": "172.17.0.1 - - [06/Jan/2017:16:16:37 +0000] \"GET /datadoghq/company?test=var1%20Pl HTTP/1.1\" 200 612 \"http://www.perdu.com/\" \"Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36\" \"-\" those are random characters" }),
        json!({ "source" : "apache", "message": "2017/09/26 14:36:50 [error] 8409#8409: *317058 \"/usr/share/nginx/html/sql/sql-admin/index.html\" is not found (2: No such file or directory), client: 217.92.148.44, server: localhost, request: \"HEAD http://174.138.82.103:80/sql/sql-admin/ HTTP/1.1\", host: \"174.138.82.103\"" }),
        json!({ "source" : "nginx", "message": "2017/09/26 14:36:50 [info] 14#14: *285 client 172.17.0.27 closed keepalive connection" }),
        json!({ "source" : "apache", "message": "127.0.0.1 - - [19/Feb/2015:15:50:36 -0500] \"GET /big.pdf HTTP/1.1\" 206 33973115 0.202 \"-\" \"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_10_1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/40.0.2214.111 Safari/537.36\"" }),
    ];
    raw.into_iter()
        .map(|json| Event::try_from(json).unwrap())
        .collect()
}
