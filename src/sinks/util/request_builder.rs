/// Generalized interface for defining how a batch of events will be turned into an request.
pub trait RequestBuilder<Input> {
    type Metadata;
    type Events;
    type Payload;
    type Request;
    type SplitError;

    /// Splits apart the input into the metadata and events portions.
    ///
    /// The metadata should be any information that needs to be passed back to `build_request`
    /// as-is, such as event finalizers, while the events are the actual events to process.
    fn split_input(&self, input: Input) -> Result<(Self::Metadata, Self::Events), Self::SplitError>;

    /// Builds a request for the given metadata and payload.
    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request;
}

pub type RequestBuilderResult<T, S> = Result<T, RequestBuilderError<S>>;

#[derive(Debug)]
pub enum RequestBuilderError<S> {
    SplitError(S),
    EncodingError(std::io::Error)
}
