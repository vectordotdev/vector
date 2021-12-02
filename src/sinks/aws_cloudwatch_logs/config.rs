#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CloudwatchLogsSinkConfig {
    pub group_name: Template,
    pub stream_name: Template,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    pub create_missing_group: Option<bool>,
    pub create_missing_stream: Option<bool>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<CloudwatchLogsDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
}

impl CloudwatchLogsSinkConfig {
    fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<CloudWatchLogsClient> {
        let region = (&self.region).try_into()?;

        let client = rusoto::client(proxy)?;
        let creds = self.auth.build(&region, self.assume_role.clone())?;

        let client = rusoto_core::Client::new_with_encoding(creds, client, self.compression.into());
        Ok(CloudWatchLogsClient::new_with_client(client, region))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_cloudwatch_logs")]
impl SinkConfig for CloudwatchLogsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let batch_settings = self.batch.into_batch_settings()?;
        let request = self.request.unwrap_with(&TowerRequestConfig::default());

        let log_group = self.group_name.clone();
        let log_stream = self.stream_name.clone();

        let client = self.create_client(cx.proxy())?;
        let svc = request.service(
            CloudwatchRetryLogic,
            CloudwatchLogsPartitionSvc::new(self.clone(), client.clone()),
        );

        let encoding = self.encoding.clone();
        let buffer = PartitionBuffer::new(VecBuffer::new(batch_settings.size));
        let sink = PartitionBatchSink::new(svc, buffer, batch_settings.timeout, cx.acker())
            .sink_map_err(|error| error!(message = "Fatal cloudwatchlogs sink error.", %error))
            .with_flat_map(move |event| {
                stream::iter(partition_encode(event, &encoding, &log_group, &log_stream)).map(Ok)
            });

        let healthcheck = healthcheck(self.clone(), client).boxed();

        Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_cloudwatch_logs"
    }
}

impl GenerateConfig for CloudwatchLogsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(default_config(Encoding::Json)).unwrap()
    }
}

//TODO: use `Default` instead
fn default_config(e: Encoding) -> CloudwatchLogsSinkConfig {
    CloudwatchLogsSinkConfig {
        group_name: Default::default(),
        stream_name: Default::default(),
        region: Default::default(),
        encoding: e.into(),
        create_missing_group: Default::default(),
        create_missing_stream: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        assume_role: Default::default(),
        auth: Default::default(),
    }
}
