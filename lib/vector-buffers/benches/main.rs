mod common;
mod native_events;
mod sized_records;

criterion::criterion_main!(native_events::native_events, sized_records::sized_records);
