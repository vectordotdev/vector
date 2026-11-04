//! Microsoft Entra ID tokens for SASL/OAUTHBEARER, for example to read from Azure Event Hubs
//! through its Kafka endpoint.

use std::{
    error::Error,
    sync::{Arc, PoisonError, RwLock},
    time::Duration,
};

use azure_core::{
    credentials::{AccessToken, TokenCredential},
    error::ErrorKind,
    time::OffsetDateTime,
};
use rdkafka::client::OAuthToken;
use tokio::sync::oneshot;
use vector_lib::internal_event::emit;

use super::oauth::{OAuthTokenProvider, OAuthTokenRefresh};
use crate::internal_events::KafkaOAuthTokenRefreshError;

/// `azure_identity` caches each token and gets a new one only in the last 5 minutes of its life.
const REFRESH_BEFORE_EXPIRY: Duration = Duration::from_mins(5);
/// librdkafka sees an expiry this much earlier than the real one. It asks for a new token at 80% of
/// the lifetime that it sees, so near the end of a token's life it asks after our refresh, while
/// the old token is still valid.
const ADVERTISED_EXPIRY_MARGIN: Duration = Duration::from_mins(3);
/// The shortest lifetime that librdkafka sees, so that it asks again soon for a token that is
/// about to expire.
const MIN_ADVERTISED_LIFETIME: Duration = Duration::from_secs(10);
/// The shortest time between two token requests.
const MIN_REFRESH_DELAY: Duration = Duration::from_secs(10);
/// The shortest time between two token requests while the credential returns the token that the
/// provider already has. Near the expiry, a managed identity can return its cached token, and the
/// Azure instance metadata service allows each VM only 20 managed identity requests per second and
/// 5 at a time, for all clients on the VM.
const SAME_TOKEN_RETRY_DELAY: Duration = Duration::from_secs(30);
/// The longest time between two attempts after errors.
const MAX_RETRY_DELAY: Duration = Duration::from_mins(1);
/// librdkafka requires a principal name. Event Hubs gets the identity from the token.
const PRINCIPAL_NAME: &str = "vector";

type TokenCache = Arc<RwLock<Option<AccessToken>>>;

/// Returns the current time. Tests replace the wall clock with Tokio's paused clock.
type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

/// Answers token requests from librdkafka with the token that [`refresh_tokens`] keeps fresh.
pub(super) struct AzureTokenProvider {
    token: TokenCache,
    now: Clock,
    /// Dropping the provider drops this sender, which stops the refresh task.
    _stop: oneshot::Sender<()>,
}

impl AzureTokenProvider {
    /// Creates the provider and the refresh that gets its tokens for `scope`, from the credential
    /// that `create_credential` returns.
    ///
    /// Nothing happens until the refresh starts. Then it creates the credential and requests the
    /// first token. The refresh task stops when the provider is dropped.
    pub(super) fn new<F>(create_credential: F, scope: String) -> (Self, OAuthTokenRefresh)
    where
        F: Fn() -> azure_core::Result<Arc<dyn TokenCredential>> + Send + Sync + 'static,
    {
        Self::with_clock(create_credential, scope, Arc::new(OffsetDateTime::now_utc))
    }

    /// Same as [`AzureTokenProvider::new`], with the clock that the provider and its refresh read.
    fn with_clock<F>(create_credential: F, scope: String, now: Clock) -> (Self, OAuthTokenRefresh)
    where
        F: Fn() -> azure_core::Result<Arc<dyn TokenCredential>> + Send + Sync + 'static,
    {
        let token = TokenCache::default();
        let (stop, stopped) = oneshot::channel();
        let (got_first_token, first_token) = oneshot::channel();
        let task = refresh_tokens(
            create_credential,
            scope,
            Arc::clone(&token),
            Arc::clone(&now),
            got_first_token,
            stopped,
        );
        let refresh = OAuthTokenRefresh {
            task: Box::pin(task),
            first_token,
        };
        (
            Self {
                token,
                now,
                _stop: stop,
            },
            refresh,
        )
    }
}

impl OAuthTokenProvider for AzureTokenProvider {
    fn token(&self) -> Result<OAuthToken, Box<dyn Error>> {
        let cached = self.token.read().unwrap_or_else(PoisonError::into_inner);
        let token = cached
            .as_ref()
            .ok_or("the first Microsoft Entra ID token is not available yet")?;
        oauth_token(token, (self.now)())
    }
}

/// Converts `token` to the form that librdkafka takes.
///
/// The expiry that librdkafka sees is [`ADVERTISED_EXPIRY_MARGIN`] before the real one, but at
/// least [`MIN_ADVERTISED_LIFETIME`] after `now`. It is never after the real expiry.
fn oauth_token(token: &AccessToken, now: OffsetDateTime) -> Result<OAuthToken, Box<dyn Error>> {
    let now = unix_millis(now);
    let expires = unix_millis(token.expires_on);
    if expires <= now {
        return Err(
            "the Microsoft Entra ID token expired and a new token is not available yet".into(),
        );
    }
    let lifetime_ms = expires
        .saturating_sub(millis(ADVERTISED_EXPIRY_MARGIN))
        .max(now.saturating_add(millis(MIN_ADVERTISED_LIFETIME)))
        .min(expires);
    Ok(OAuthToken {
        token: token.token.secret().to_owned(),
        principal_name: PRINCIPAL_NAME.to_owned(),
        lifetime_ms,
    })
}

/// Creates the credential and requests a token at once, then requests a new token before each
/// token expires, until `stopped` completes. Sends on `got_first_token` when the cache gets its
/// first token.
///
/// After an error, the old token stays in the cache while it is valid, and the delay before the
/// next attempt doubles up to [`MAX_RETRY_DELAY`].
async fn refresh_tokens(
    create_credential: impl Fn() -> azure_core::Result<Arc<dyn TokenCredential>>,
    scope: String,
    cache: TokenCache,
    now: Clock,
    got_first_token: oneshot::Sender<()>,
    mut stopped: oneshot::Receiver<()>,
) {
    let mut got_first_token = Some(got_first_token);
    let mut credential = None;
    let mut cached_expiry = None;
    let mut retry_delay = MIN_REFRESH_DELAY;
    loop {
        let result = tokio::select! {
            _ = &mut stopped => return,
            result = request_token(&create_credential, &mut credential, &scope) => result,
        };
        let delay = match result {
            Ok(token) => {
                retry_delay = MIN_REFRESH_DELAY;
                let renewed = cached_expiry != Some(token.expires_on);
                cached_expiry = Some(token.expires_on);
                let delay = refresh_delay(token.expires_on, now(), renewed);
                *cache.write().unwrap_or_else(PoisonError::into_inner) = Some(token);
                if let Some(got_first_token) = got_first_token.take() {
                    _ = got_first_token.send(());
                }
                delay
            }
            Err(error) => {
                emit(KafkaOAuthTokenRefreshError { error: &error });
                let delay = retry_delay;
                retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
                delay
            }
        };
        tokio::select! {
            _ = &mut stopped => return,
            () = tokio::time::sleep(delay) => {}
        }
    }
}

/// Requests a token for `scope`, and first creates the credential if it does not exist yet.
async fn request_token(
    create_credential: &impl Fn() -> azure_core::Result<Arc<dyn TokenCredential>>,
    credential: &mut Option<Arc<dyn TokenCredential>>,
    scope: &str,
) -> azure_core::Result<AccessToken> {
    let credential = match credential {
        Some(credential) => credential,
        None => credential.insert(create_credential()?),
    };
    let token = credential.get_token(&[scope], None).await?;
    if !is_b64token(token.token.secret()) {
        return Err(azure_core::Error::with_message(
            ErrorKind::DataConversion,
            "the Microsoft Entra ID token has characters that a bearer token cannot have",
        ));
    }
    Ok(token)
}

/// Returns whether `token` is a `b64token` (RFC 6750, section 2.1), as every Microsoft Entra ID
/// access token is.
///
/// librdkafka rejects a token with some other characters, and then copies the whole token into its
/// error message, which the component logs.
fn is_b64token(token: &str) -> bool {
    let token = token.trim_end_matches('=');
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte))
}

/// Returns the time to wait before the next request after getting a token that expires at
/// `expires_on`.
///
/// If the token is not `renewed`, the credential returned the token that the provider already had,
/// so the next request waits about 30 seconds ([`SAME_TOKEN_RETRY_DELAY`]), capped at the remaining
/// lifetime, with a 10 second floor ([`MIN_REFRESH_DELAY`]).
fn refresh_delay(expires_on: OffsetDateTime, now: OffsetDateTime, renewed: bool) -> Duration {
    let lifetime = Duration::from_millis(
        u64::try_from(unix_millis(expires_on).saturating_sub(unix_millis(now))).unwrap_or(0),
    );
    let mut delay = lifetime.saturating_sub(REFRESH_BEFORE_EXPIRY);
    if !renewed {
        delay = delay.max(SAME_TOKEN_RETRY_DELAY.min(lifetime));
    }
    delay.max(MIN_REFRESH_DELAY)
}

fn unix_millis(time: OffsetDateTime) -> i64 {
    i64::try_from(time.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        fs::File,
        io::{Read, Seek},
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use azure_core::credentials::TokenRequestOptions;
    use vector_lib::event_test_util;

    use super::*;

    const SCOPE: &str = "https://mynamespace.servicebus.windows.net/.default";

    /// Answers token requests with the given responses in order, then never answers again.
    #[derive(Debug)]
    struct ScriptedCredential {
        responses: Mutex<VecDeque<azure_core::Result<AccessToken>>>,
        requests: AtomicUsize,
    }

    impl ScriptedCredential {
        fn new(responses: impl IntoIterator<Item = azure_core::Result<AccessToken>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses.into_iter().collect()),
                requests: AtomicUsize::new(0),
            })
        }

        fn requests(&self) -> usize {
            self.requests.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl TokenCredential for ScriptedCredential {
        async fn get_token(
            &self,
            scopes: &[&str],
            _options: Option<TokenRequestOptions<'_>>,
        ) -> azure_core::Result<AccessToken> {
            assert_eq!(scopes, [SCOPE]);
            self.requests.fetch_add(1, Ordering::SeqCst);
            let response = self.responses.lock().unwrap().pop_front();
            match response {
                Some(response) => response,
                None => std::future::pending().await,
            }
        }
    }

    /// The time at which each provider in these tests starts.
    const START: i64 = 1_700_000_000;

    /// Returns a clock that starts at [`START`] and follows Tokio's clock, so that the timing of
    /// tests with paused time does not depend on the wall clock.
    fn test_clock() -> Clock {
        let started = tokio::time::Instant::now();
        Arc::new(move || at(START) + started.elapsed())
    }

    fn token(secret: &'static str, expires_in: Duration) -> AccessToken {
        AccessToken::new(secret, at(START) + expires_in)
    }

    fn request_failed() -> azure_core::Result<AccessToken> {
        Err(azure_core::Error::with_message(
            ErrorKind::Credential,
            "token request failed",
        ))
    }

    /// Returns the error that a workload identity credential returns after a refused connection.
    /// Like other Azure SDK errors, each error in it prints only its own message.
    fn connection_refused() -> azure_core::Error {
        let transport = azure_core::Error::with_error(
            ErrorKind::Io,
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused"),
            "failed to execute `reqwest` request",
        );
        let retry = transport
            .with_context("retry policy expired and the request will no longer be retried");
        let message = format!("WorkloadIdentityCredential authentication failed. {retry}");
        azure_core::Error::with_error(ErrorKind::Credential, retry, message)
    }

    /// Writes what the current thread logs to the returned file while the guard lives.
    fn capture_logs() -> (File, tracing::subscriber::DefaultGuard) {
        let logs = tempfile::tempfile().unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(logs.try_clone().unwrap())
            .finish();
        (logs, tracing::subscriber::set_default(subscriber))
    }

    fn read_logs(mut logs: &File) -> String {
        let mut text = String::new();
        logs.rewind().unwrap();
        logs.read_to_string(&mut text).unwrap();
        text
    }

    /// Creates a provider that gets its tokens from `credential`.
    fn new_provider(
        credential: &Arc<ScriptedCredential>,
    ) -> (AzureTokenProvider, OAuthTokenRefresh) {
        let credential = Arc::clone(credential);
        AzureTokenProvider::with_clock(
            move || Ok(Arc::clone(&credential) as Arc<dyn TokenCredential>),
            SCOPE.to_owned(),
            test_clock(),
        )
    }

    fn cached_token(provider: &AzureTokenProvider) -> String {
        provider
            .token()
            .expect("the provider should have a token")
            .token
    }

    fn at(unix_seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(unix_seconds).unwrap()
    }

    #[test]
    fn librdkafka_sees_an_expiry_before_the_real_one() {
        let now = at(1_700_000_000);
        let access_token = AccessToken::new("test-access-token", now + Duration::from_hours(1));

        let token = oauth_token(&access_token, now).unwrap();
        assert_eq!(token.token, "test-access-token");
        assert_ne!(token.principal_name, "");
        assert_eq!(token.lifetime_ms, (1_700_000_000 + 3600 - 180) * 1000);
    }

    #[test]
    fn librdkafka_never_sees_an_expiry_after_the_real_one() {
        let now = at(1_700_000_000);

        // 100 seconds left: librdkafka sees 10 seconds, so it asks again soon.
        let access_token = AccessToken::new("test-access-token", now + Duration::from_secs(100));
        let token = oauth_token(&access_token, now).unwrap();
        assert_eq!(token.lifetime_ms, (1_700_000_000 + 10) * 1000);

        // 5 seconds left: librdkafka sees the real expiry.
        let access_token = AccessToken::new("test-access-token", now + Duration::from_secs(5));
        let token = oauth_token(&access_token, now).unwrap();
        assert_eq!(token.lifetime_ms, (1_700_000_000 + 5) * 1000);
    }

    #[test]
    fn expired_token_is_an_error() {
        let now = at(1_700_000_000);
        for expires_on in [now, now - Duration::from_secs(1)] {
            let access_token = AccessToken::new("test-access-token", expires_on);
            let error = oauth_token(&access_token, now)
                .err()
                .expect("an expired token should be an error")
                .to_string();
            assert!(error.contains("expired"), "{error}");
            assert!(!error.contains("test-access-token"), "{error}");
        }
    }

    #[test]
    fn refresh_starts_five_minutes_before_expiry() {
        let now = at(1_700_000_000);
        assert_eq!(
            refresh_delay(now + Duration::from_hours(1), now, true),
            Duration::from_mins(55)
        );
        assert_eq!(
            refresh_delay(now + Duration::from_secs(301), now, true),
            MIN_REFRESH_DELAY
        );
        assert_eq!(
            refresh_delay(now - Duration::from_mins(1), now, true),
            MIN_REFRESH_DELAY
        );
    }

    #[test]
    fn refresh_waits_longer_after_the_same_token() {
        // 30 seconds, but not past the expiry, and not less than 10 seconds.
        let now = at(1_700_000_000);
        for (expires_in, delay) in [
            (Duration::from_mins(4), SAME_TOKEN_RETRY_DELAY),
            (Duration::from_secs(20), Duration::from_secs(20)),
            (Duration::from_secs(5), MIN_REFRESH_DELAY),
            (Duration::from_hours(1), Duration::from_mins(55)),
        ] {
            assert_eq!(
                refresh_delay(now + expires_in, now, false),
                delay,
                "{expires_in:?}"
            );
        }
    }

    #[test]
    fn tokens_must_be_b64tokens() {
        for token in [
            "eyJ0eXAiOiJKV1QifQ.eyJhdWQiOiJ4In0.c2lnbmF0dXJl-_~+/",
            "dG9rZW4=",
            "dG9rZQ==",
        ] {
            assert!(is_b64token(token), "{token}");
        }
        for token in [
            "", "==", "to ken", "to=ken", "to\"ken", "to,ken", "to\nken", "tök", "to\0ken",
        ] {
            assert!(!is_b64token(token), "{token:?}");
        }
    }

    #[tokio::test]
    async fn refresh_creates_the_credential_when_it_starts() {
        let credential = ScriptedCredential::new([Ok(token("first", Duration::from_hours(1)))]);
        let credentials_created = Arc::new(AtomicUsize::new(0));
        let (provider, refresh) = {
            let credential = Arc::clone(&credential);
            let credentials_created = Arc::clone(&credentials_created);
            AzureTokenProvider::with_clock(
                move || {
                    credentials_created.fetch_add(1, Ordering::SeqCst);
                    Ok(Arc::clone(&credential) as Arc<dyn TokenCredential>)
                },
                SCOPE.to_owned(),
                test_clock(),
            )
        };

        // Creating the provider neither creates the credential nor requests a token.
        let error = provider
            .token()
            .err()
            .expect("the provider should have no token before the refresh starts")
            .to_string();
        assert!(error.contains("not available yet"), "{error}");
        assert_eq!(credentials_created.load(Ordering::SeqCst), 0);
        assert_eq!(credential.requests(), 0);

        // `start` returns when the provider has its first token.
        refresh.start().await;
        assert_eq!(cached_token(&provider), "first");
        assert_eq!(credentials_created.load(Ordering::SeqCst), 1);
        assert_eq!(credential.requests(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_retries_when_the_credential_cannot_be_created() {
        event_test_util::clear_recorded_events();
        let credential = ScriptedCredential::new([Ok(token("first", Duration::from_hours(1)))]);
        let failures = Arc::new(AtomicUsize::new(2));
        let (provider, refresh) = {
            let credential = Arc::clone(&credential);
            let failures = Arc::clone(&failures);
            AzureTokenProvider::with_clock(
                move || {
                    if failures.load(Ordering::SeqCst) > 0 {
                        failures.fetch_sub(1, Ordering::SeqCst);
                        return Err(azure_core::Error::with_message(
                            ErrorKind::Credential,
                            "failed to read federated token from file /nonexistent",
                        ));
                    }
                    Ok(Arc::clone(&credential) as Arc<dyn TokenCredential>)
                },
                SCOPE.to_owned(),
                test_clock(),
            )
        };
        drop(tokio::spawn(refresh.task));

        // The first two attempts fail, then the next ones come after 10 and 20 seconds.
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(provider.token().is_err());
        event_test_util::contains_name_once("KafkaOAuthTokenRefreshError").unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert!(provider.token().is_err());
        assert_eq!(credential.requests(), 0);
        tokio::time::sleep(Duration::from_secs(20)).await;
        assert_eq!(cached_token(&provider), "first");
        assert_eq!(credential.requests(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_gets_a_new_token_before_expiry() {
        let credential = ScriptedCredential::new([
            Ok(token("first", Duration::from_hours(1))),
            Ok(token("second", Duration::from_hours(2))),
        ]);
        let (provider, refresh) = new_provider(&credential);
        let task = tokio::spawn(refresh.task);

        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(credential.requests(), 1);
        assert_eq!(cached_token(&provider), "first");

        // The next request comes 5 minutes before the first token expires.
        tokio::time::sleep(Duration::from_secs(3290)).await;
        assert_eq!(credential.requests(), 1);
        tokio::time::sleep(Duration::from_secs(20)).await;
        assert_eq!(credential.requests(), 2);
        assert_eq!(cached_token(&provider), "second");

        drop(provider);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("the refresh task should stop when the provider is dropped")
            .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_keeps_the_token_and_backs_off_after_errors() {
        event_test_util::clear_recorded_events();
        let credential = ScriptedCredential::new([
            Ok(token("first", Duration::from_mins(10))),
            request_failed(),
            request_failed(),
            request_failed(),
            request_failed(),
            request_failed(),
            Ok(token("second", Duration::from_hours(1))),
        ]);
        let (provider, refresh) = new_provider(&credential);
        drop(tokio::spawn(refresh.task));

        // The first token is refreshed after 300 seconds. The requests after that fail, and the
        // delays between them double to the maximum: 10, 20, 40, 60, and 60 seconds.
        for (wait, requests) in [
            (301, 2),
            (10, 3),
            (20, 4),
            (40, 5),
            (58, 5),
            (2, 6),
            (60, 7),
        ] {
            tokio::time::sleep(Duration::from_secs(wait)).await;
            assert_eq!(credential.requests(), requests, "after {wait} more seconds");
            if requests < 7 {
                assert_eq!(cached_token(&provider), "first");
            }
        }
        assert_eq!(cached_token(&provider), "second");
        event_test_util::contains_name_once("KafkaOAuthTokenRefreshError").unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn provider_rejects_an_expired_cached_token() {
        let credential = ScriptedCredential::new([Ok(AccessToken::new(
            "test-access-token",
            at(START) - Duration::from_secs(1),
        ))]);
        let (provider, refresh) = new_provider(&credential);
        drop(tokio::spawn(refresh.task));

        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(credential.requests(), 1);
        let error = provider
            .token()
            .err()
            .expect("an expired token should be an error")
            .to_string();
        assert!(error.contains("expired"), "{error}");
        assert!(!error.contains("test-access-token"), "{error}");
    }

    #[tokio::test(start_paused = true)]
    async fn start_waits_for_the_first_token_at_most_ten_seconds() {
        // The credential never answers, so `start` gives up after 10 seconds.
        let credential = ScriptedCredential::new([]);
        let (provider, refresh) = new_provider(&credential);
        let started = tokio::time::Instant::now();
        refresh.start().await;
        let waited = started.elapsed();
        assert!(
            (Duration::from_secs(10)..Duration::from_secs(11)).contains(&waited),
            "waited {waited:?}"
        );
        assert_eq!(credential.requests(), 1);
        assert!(provider.token().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_backs_off_while_the_credential_returns_the_same_token() {
        // A managed identity can return its cached token until shortly before it expires.
        let cached = token("first", Duration::from_mins(4));
        let credential = ScriptedCredential::new([
            Ok(cached.clone()),
            Ok(cached.clone()),
            Ok(cached),
            Ok(token("second", Duration::from_hours(1))),
        ]);
        let (provider, refresh) = new_provider(&credential);
        drop(tokio::spawn(refresh.task));

        // The first token expires in less than 5 minutes, so the next request comes after 10
        // seconds. It gets the same token, so the requests after it come every 30 seconds.
        for (wait, requests) in [(1, 1), (10, 2), (28, 2), (2, 3), (30, 4)] {
            tokio::time::sleep(Duration::from_secs(wait)).await;
            assert_eq!(credential.requests(), requests, "after {wait} more seconds");
        }
        assert_eq!(cached_token(&provider), "second");
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_rejects_a_token_that_is_not_a_b64token() {
        let (logs, _guard) = capture_logs();
        event_test_util::clear_recorded_events();
        let credential = ScriptedCredential::new([
            Ok(token("not a bearer token", Duration::from_hours(1))),
            Ok(token("second", Duration::from_hours(1))),
        ]);
        let (provider, refresh) = new_provider(&credential);
        drop(tokio::spawn(refresh.task));

        // The provider does not keep the token, and no log contains it.
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(provider.token().is_err());
        event_test_util::contains_name_once("KafkaOAuthTokenRefreshError").unwrap();
        let text = read_logs(&logs);
        assert!(
            text.contains("characters that a bearer token cannot have"),
            "{text}"
        );
        assert!(!text.contains("not a bearer token"), "{text}");

        // The next request comes after 10 seconds.
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(cached_token(&provider), "second");
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_errors_show_their_causes_but_no_token() {
        let (logs, _guard) = capture_logs();
        let credential = ScriptedCredential::new([
            Ok(token("test-access-token", Duration::from_mins(10))),
            Err(connection_refused()),
        ]);
        let (provider, refresh) = new_provider(&credential);
        drop(tokio::spawn(refresh.task));

        // The refresh 5 minutes before the token expires fails.
        tokio::time::sleep(Duration::from_secs(301)).await;
        assert_eq!(credential.requests(), 2);
        assert_eq!(cached_token(&provider), "test-access-token");
        let text = read_logs(&logs);
        assert!(
            text.contains(
                "WorkloadIdentityCredential authentication failed. retry policy expired and the \
                 request will no longer be retried: failed to execute `reqwest` request: \
                 connection refused"
            ),
            "{text}"
        );
        assert_eq!(text.matches("retry policy expired").count(), 1, "{text}");
        assert!(text.contains("oauth_token_refresh"), "{text}");
        assert!(!text.contains("test-access-token"), "{text}");
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_stops_when_the_provider_is_dropped() {
        // While a token request never answers.
        let credential = ScriptedCredential::new([]);
        let (provider, refresh) = new_provider(&credential);
        let task = tokio::spawn(refresh.task);
        tokio::time::sleep(Duration::from_mins(10)).await;
        assert_eq!(credential.requests(), 1);
        assert!(provider.token().is_err());
        assert!(!task.is_finished());

        drop(provider);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("the refresh task should stop when the provider is dropped")
            .unwrap();

        // While the task waits for the next refresh.
        let credential = ScriptedCredential::new([Ok(token("first", Duration::from_hours(1)))]);
        let (provider, refresh) = new_provider(&credential);
        let task = tokio::spawn(refresh.task);
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(cached_token(&provider), "first");
        assert!(!task.is_finished());

        drop(provider);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("the refresh task should stop when the provider is dropped")
            .unwrap();
        assert_eq!(credential.requests(), 1);
    }
}
