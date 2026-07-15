#[derive(Debug, snafu::Snafu)]
#[snafu(visibility(pub))]
pub enum VectorSinkError {
    #[snafu(display("Request failed: {}", source))]
    Request { source: tonic::Status },

    #[snafu(display("Vector source unhealthy: {:?}", status))]
    Health { status: Option<&'static str> },

    #[snafu(display("URL has no host."))]
    NoHost,
}

