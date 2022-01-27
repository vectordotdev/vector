use std::io::Cursor;

use super::SizedRecord;
use crate::disk_v2::{reader::RecordReader, writer::RecordWriter};

#[tokio::test]
async fn roundtrip_through_record_writer_and_record_reader() {
    // Create a duplex stream that's more than big enough to ship a record through.
    let (writer_io, reader_io) = tokio::io::duplex(4096);

    let mut record_writer = RecordWriter::new(writer_io, 2048);
    let mut record_reader = RecordReader::new(reader_io);

    let record = SizedRecord(73);

    let bytes_written = record_writer
        .write_record(314, record.clone())
        .await
        .expect("write should not fail");
    record_writer.flush().await.expect("flush should not fail");

    let read_token = record_reader
        .try_next_record(false)
        .await
        .expect("read should not fail");
    assert!(read_token.is_some());

    let read_token = read_token.unwrap();
    assert_eq!(bytes_written, read_token.record_len());
    assert_eq!(314, read_token.record_id());

    let roundtrip_record = record_reader
        .read_record(read_token)
        .expect("read should not fail");
    assert_eq!(record, roundtrip_record);
}

#[tokio::test]
async fn record_reader_always_returns_none_when_no_data() {
    let reader_io = Cursor::new(Vec::new());

    let mut record_reader = RecordReader::<_, SizedRecord>::new(reader_io);
    let read_token = record_reader
        .try_next_record(false)
        .await
        .expect("read should not fail");
    assert!(read_token.is_none());
}
