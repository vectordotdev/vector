#[derive(Debug, Clone)]
struct CloudwatchRetryLogic;

impl RetryLogic for CloudwatchRetryLogic {
    type Error = CloudwatchError;
    type Response = ();

    #[allow(clippy::cognitive_complexity)] // long, but just a hair over our limit
    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            CloudwatchError::Put(err) => match err {
                RusotoError::Service(PutLogEventsError::ServiceUnavailable(error)) => {
                    error!(message = "Put logs service unavailable.", %error);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "Put logs HTTP dispatch.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "Put logs HTTP error.", status = %status, body = %body);
                    true
                }

                RusotoError::Unknown(res)
                    if rusoto_core::proto::json::Error::parse(res)
                        .filter(|error| error.typ.as_str() == "ThrottlingException")
                        .is_some() =>
                {
                    true
                }

                _ => false,
            },

            CloudwatchError::Describe(err) => match err {
                RusotoError::Service(DescribeLogStreamsError::ServiceUnavailable(error)) => {
                    error!(message = "Describe streams service unavailable.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "Describe streams HTTP error.", status = %status, body = %body);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "Describe streams HTTP dispatch.", %error);
                    true
                }

                _ => false,
            },

            CloudwatchError::CreateStream(err) => match err {
                RusotoError::Service(CreateLogStreamError::ServiceUnavailable(error)) => {
                    error!(message = "Create stream service unavailable.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "Create stream HTTP error.", status = %status, body = %body);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "Create stream HTTP dispatch.", %error);
                    true
                }

                _ => false,
            },
            _ => false,
        }
    }
}
