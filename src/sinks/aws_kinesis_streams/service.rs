#[derive(Clone)]
pub struct KinesisService {
    client: KinesisClient,
    config: KinesisSinkConfig,
}

impl KinesisService {
    pub fn new(
        config: KinesisSinkConfig,
        client: KinesisClient,
        cx: SinkContext,
    ) -> crate::Result<impl Sink<Event, Error = ()>> {
        let batch = BatchSettings::default()
            .bytes(5_000_000)
            .events(500)
            .timeout(1)
            .parse_config(config.batch)?;
        let request = config.request.unwrap_with(&TowerRequestConfig::default());
        let encoding = config.encoding.clone();
        let partition_key_field = config.partition_key_field.clone();

        let kinesis = KinesisService { client, config };

        let sink = request
            .batch_sink(
                KinesisRetryLogic,
                kinesis,
                VecBuffer::new(batch.size),
                batch.timeout,
                cx.acker(),
                sink::StdServiceLogic::default(),
            )
            .sink_map_err(|error| error!(message = "Fatal kinesis streams sink error.", %error))
            .with_flat_map(move |e| {
                stream::iter(encode_event(e, &partition_key_field, &encoding)).map(Ok)
            });

        Ok(sink)
    }
}

impl Service<Vec<PutRecordsRequestEntry>> for KinesisService {
    type Response = PutRecordsOutput;
    type Error = RusotoError<PutRecordsError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, records: Vec<PutRecordsRequestEntry>) -> Self::Future {
        debug!(
            message = "Sending records.",
            events = %records.len(),
        );

        let sizes: Vec<usize> = records.iter().map(|record| record.data.len()).collect();

        let client = self.client.clone();
        let request = PutRecordsInput {
            records,
            stream_name: self.config.stream_name.clone(),
        };

        Box::pin(async move {
            client
                .put_records(request)
                .inspect_ok(|_| {
                    for byte_size in sizes {
                        emit!(&AwsKinesisStreamsEventSent { byte_size });
                    }
                })
                .instrument(info_span!("request"))
                .await
        })
    }
}

impl fmt::Debug for KinesisService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KinesisService")
            .field("config", &self.config)
            .finish()
    }
}
