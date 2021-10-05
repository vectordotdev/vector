use crate::sinks::util::RequestBuilder;
use crate::event::Event;

pub struct ElasticsearchRequestBuilder {

}

impl RequestBuilder<Vec<Event>> for ElasticsearchRequestBuilder {
    type Metadata = ();
    type Events = ();
    type Payload = ();
    type Request = ();
    type SplitError = ();

    fn split_input(&self, input: Vec<Event>) -> Result<(Self::Metadata, Self::Events), Self::SplitError> {
        todo!()
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        todo!()
    }
}
