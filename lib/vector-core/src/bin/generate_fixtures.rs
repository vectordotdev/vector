use std::{fs::File, io::Write, path::PathBuf};

use bytes::BytesMut;
use prost::Message;
use quickcheck::{Arbitrary as _, Gen};
use vector_core::event::{Event, EventArray, proto};

const SEED: u64 = 0;
const GEN_SIZE: usize = 128;

fn main() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../codecs/tests/data/native_encoding");
    let json_dir = fixture_dir.join("json");
    let proto_dir = fixture_dir.join("proto");
    std::fs::create_dir_all(&json_dir).unwrap();
    std::fs::create_dir_all(&proto_dir).unwrap();

    let mut rng = Gen::from_size_and_seed(GEN_SIZE, SEED);
    for n in 0..1024_usize {
        let event = Event::arbitrary(&mut rng);

        let mut json_out = File::create(json_dir.join(format!("{n:04}.json"))).unwrap();
        serde_json::to_writer(&mut json_out, &event).unwrap();

        let mut proto_out = File::create(proto_dir.join(format!("{n:04}.pb"))).unwrap();
        let mut buf = BytesMut::new();
        proto::EventArray::from(EventArray::from(event))
            .encode(&mut buf)
            .unwrap();
        proto_out.write_all(&buf).unwrap();
    }

    #[allow(clippy::print_stdout)]
    {
        println!("Written 1024 fixtures to {}", fixture_dir.display());
    }
}
