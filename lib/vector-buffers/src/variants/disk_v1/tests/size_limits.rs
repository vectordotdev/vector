use crate::{
    assert_reader_writer_v1_positions,
    test::common::{with_temp_dir, SizedRecord},
    variants::disk_v1::tests::create_default_buffer_v1_with_max_buffer_size,
};

#[tokio::test]
async fn ensure_write_offset_valid_after_reload_with_multievent() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create our buffer with and arbitrarily low max buffer size, and two write sizes that
            // will both fit just under the limit but will provide no chance for another write to
            // fit.
            //
            // The sizes are different so that we can assert that we got back the expected record at
            // each read we perform.
            let (mut writer, reader, _) =
                create_default_buffer_v1_with_max_buffer_size(data_dir.clone(), 100);
            let first_write_size = 92;
            let second_write_size = 96;

            assert_reader_writer_v1_positions!(reader, writer, 0, 0);

            // First write should always complete because we haven't written anything yet, so we
            // haven't exceed our total buffer size limit yet, or the size limit of the data file
            // itself.  We do need this write to be big enough to exceed the total buffer size
            // limit, though.
            let first_record = SizedRecord(first_write_size);
            let first_write_result = writer.try_send(first_record);
            assert_eq!(first_write_result, None);
            writer.flush();

            // This write should return immediately because will have exceeded our 100 byte total
            // buffer size limit handily with the first write we did, but since it's a fallible
            // write attempt, it can already tell that the write will not fit anyways:
            let record = SizedRecord(second_write_size);
            let second_write_result = writer.try_send(record.clone());

            assert_eq!(second_write_result, Some(record));
        }
    })
    .await;
}
