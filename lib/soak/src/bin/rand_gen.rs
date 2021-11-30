use rand::Rng;
use soak::{Output, Unit};
use std::borrow::Cow;

#[derive(argh::FromArgs)]
/// vector soak `rand_gen` options
struct Opts {
    /// total number of samples to write to captures file
    #[argh(option)]
    total_samples: u16,
    /// the name of the experiment
    #[argh(option)]
    experiment: String,
    /// the variant of the experiment
    #[argh(option)]
    variant: soak::Variant,
}

fn main() {
    tracing_subscriber::fmt().init();
    let ops: Opts = argh::from_env();

    let mut rng = rand::thread_rng();

    let unit = Unit::Bytes;
    let query_id = "throughput";
    let query = "sum(rate(bytes_written[2s]))";
    let vector_id = "rand_gen-SHA";
    let experiment = ops.experiment;
    let variant = ops.variant;

    let mut wtr = csv::Writer::from_writer(vec![]);
    let mut time = 0.0;
    for idx in 0..ops.total_samples {
        let output: soak::Output = Output {
            experiment: Cow::Borrowed(&experiment),
            variant,
            vector_id: Cow::Borrowed(vector_id),
            time,
            fetch_index: idx as u64,
            query: Cow::Borrowed(query),
            query_id: Cow::Borrowed(query_id),
            value: rng.gen(),
            unit,
        };
        time += 1.0;
        wtr.serialize(&output).unwrap();
    }
    let data = String::from_utf8(wtr.into_inner().unwrap()).unwrap();
    println!("{}", data);
}
