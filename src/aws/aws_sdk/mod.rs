use aws_smithy_client::SdkError;
use once_cell::sync::OnceCell;
use regex::RegexSet;

mod auth;
mod region;

static RETRIABLE_CODES: OnceCell<RegexSet> = OnceCell::new();

pub fn is_retriable_error<T>(error: &SdkError<T>) -> bool {
    match error {
        SdkError::TimeoutError(_) => true,
        SdkError::DispatchFailure(_) => true,
        SdkError::ResponseError { err: _, raw } | SdkError::ServiceError { err: _, raw } => {
            // This header is a direct indication that we should retry the request. Eventually it'd
            // be nice to actually schedule the retry after the given delay, but for now we just
            // check that it contains a positive value.
            let retry_header = raw.http().headers().get("x-amz-retry-after").is_some();

            // Certain 400-level responses will contain an error code indicating that the request
            // should be retried. Since we don't retry 400-level responses by default, we'll look
            // for these specifically before falling back to more general heuristics. Because AWS
            // services use a mix of XML and JSON response bodies and Rusoto doesn't give us
            // a parsed representation, we resort to a simple string match.
            //
            // S3: RequestTimeout
            // SQS: RequestExpired, ThrottlingException
            // ECS: RequestExpired, ThrottlingException
            // Kinesis: RequestExpired, ThrottlingException
            // Cloudwatch: RequestExpired, ThrottlingException
            //
            // Now just look for those when it's a client_error
            let re = RETRIABLE_CODES.get_or_init(|| {
                RegexSet::new(&["RequestTimeout", "RequestExpired", "ThrottlingException"])
                    .expect("invalid regex")
            });

            let status = raw.http().status();
            let response_body = String::from_utf8_lossy(raw.http().body().bytes().unwrap_or(&[]));

            retry_header
                || status.is_server_error()
                || status == http::StatusCode::TOO_MANY_REQUESTS
                || (status.is_client_error() && re.is_match(response_body.as_ref()))
        }
        _ => false,
    }
}
