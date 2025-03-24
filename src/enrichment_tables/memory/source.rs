use super::Memory;

/// A struct that represents Memory when used as a source.
pub struct MemorySource {
    memory: Memory,
    shutdown: ShutdownSignal,
    out: SourceSender,
    log_namespace: LogNamespace,
    dump_batch_size: Option<usize>,
}

impl MemorySource {
    pub(crate) async fn run(mut self) -> Result<(), ()> {
        let events_received = register!(EventsReceived);
        let bytes_received = register!(BytesReceived::from(Protocol::INTERNAL));
        let mut interval = IntervalStream::new(interval(Duration::from_secs(
            self.memory
                .config
                .dump_interval
                .expect("Unexpected missing dump interval in memory table used as a source."),
        )))
        .take_until(self.shutdown);

        while interval.next().await.is_some() {
            let mut sent = 0_usize;
            loop {
                let mut events = Vec::new();
                {
                    let mut writer = self.memory.write_handle.lock().unwrap();
                    if let Some(reader) = self.memory.get_read_handle().read() {
                        let now = Instant::now();
                        let utc_now = Utc::now();
                        events = reader
                            .iter()
                            .skip(if self.memory.config.remove_after_dump {
                                0
                            } else {
                                sent
                            })
                            .take(if let Some(batch_size) = self.dump_batch_size {
                                batch_size
                            } else {
                                usize::MAX
                            })
                            .filter_map(|(k, v)| {
                                if self.memory.config.remove_after_dump {
                                    writer.write_handle.empty(k.clone());
                                }
                                v.get_one().map(|v| (k, v))
                            })
                            .filter_map(|(k, v)| {
                                let mut event = Event::Log(LogEvent::from_map(
                                    v.as_object_map(now, self.memory.config.ttl, k).ok()?,
                                    EventMetadata::default(),
                                ));
                                let log = event.as_mut_log();
                                self.log_namespace.insert_standard_vector_source_metadata(
                                    log,
                                    MemoryConfig::NAME,
                                    utc_now,
                                );

                                Some(event)
                            })
                            .collect::<Vec<_>>();
                        if self.memory.config.remove_after_dump {
                            writer.write_handle.refresh();
                        }
                    }
                }
                let count = events.len();
                let byte_size = events.size_of();
                let json_size = events.estimated_json_encoded_size_of();
                bytes_received.emit(ByteSize(byte_size));
                events_received.emit(CountByteSize(count, json_size));
                if self.out.send_batch(events).await.is_err() {
                    emit!(StreamClosedError { count });
                }

                sent += count;
                match self.dump_batch_size {
                    None => break,
                    Some(dump_batch_size) if count < dump_batch_size => break,
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
