use std::{
    pin::Pin,
    sync::{Arc, Mutex, Weak},
};

use futures::SinkExt;
use graphql_client::GraphQLQuery;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;
use uuid::Uuid;
use weak_table::WeakValueHashMap;

/// Subscription GraphQL response, returned from an active stream.
pub type StreamResponse<T> = Pin<
    Box<
        dyn Stream<Item = Option<graphql_client::Response<<T as GraphQLQuery>::ResponseData>>>
            + Send
            + Sync,
    >,
>;

/// Payload contains the raw data received back from a GraphQL subscription. At the point
/// of receiving data, the only known fields are { id, type }; what's contained inside the
/// `payload` field is unknown until we attempt to deserialize it against a generated
/// GraphQLQuery::ResponseData later.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Payload {
    id: Uuid,
    #[serde(rename = "type")]
    payload_type: String,
    payload: serde_json::Value,
}

impl Payload {
    /// Returns an "init" payload to confirm the connection to the server.
    pub fn init(id: Uuid) -> Self {
        Self {
            id,
            payload_type: "connection_init".to_owned(),
            payload: json!({}),
        }
    }

    /// Returns a "start" payload necessary for starting a new subscription.
    pub fn start<T: GraphQLQuery + Send + Sync>(
        id: Uuid,
        payload: &graphql_client::QueryBody<T::Variables>,
    ) -> Self {
        Self {
            id,
            payload_type: "start".to_owned(),
            payload: json!(payload),
        }
    }

    /// Returns a "stop" payload for terminating the subscription in the GraphQL server.
    fn stop(id: Uuid) -> Self {
        Self {
            id,
            payload_type: "stop".to_owned(),
            payload: serde_json::Value::Null,
        }
    }

    /// Attempts to return a definitive ResponseData on the `payload` field, matched against
    /// a generated `GraphQLQuery`.
    fn response<T: GraphQLQuery + Send + Sync>(
        &self,
    ) -> Option<graphql_client::Response<T::ResponseData>> {
        serde_json::from_value::<graphql_client::Response<T::ResponseData>>(self.payload.clone())
            .ok()
    }
}

/// Receiver<T> has a single method, `stream`, that returns a `StreamResponse<T>` of
/// `Payload`s received from the server.
pub trait Receiver<T: GraphQLQuery + Send + Sync> {
    /// Returns a stream of `Payload` responses, received from the GraphQL server
    fn stream(&self) -> StreamResponse<T>;
}

/// BoxedSubscription<T> returns a thread-safe, boxed `Receiver<T>`.
pub type BoxedSubscription<T> = Box<Arc<dyn Receiver<T> + Send + Sync>>;

/// A Subscription is associated with a single GraphQL subscription query. Its methods
/// allow transmitting `Payload`s upstream to the API server, via its `Receiver<T: GraphQLQuery`
/// implementation, returning a stream of `Payload`, and preventing additional `Payload`
/// messages from the server.
#[derive(Debug)]
pub struct Subscription {
    id: Uuid,
    tx: broadcast::Sender<Payload>,
    client_tx: tokio::sync::mpsc::UnboundedSender<Payload>,
}

impl Subscription {
    /// Returns a new Subscription, that is associated with a particular GraphQL query.
    pub fn new(id: Uuid, client_tx: tokio::sync::mpsc::UnboundedSender<Payload>) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { id, tx, client_tx }
    }

    // Initialize the connection by sending a "GQL_CONNECTION_INIT" message.
    fn init(&self) -> Result<(), tokio::sync::mpsc::error::SendError<Payload>> {
        self.client_tx.send(Payload::init(self.id))
    }

    /// Send a payload down the channel. This is synchronous because broadcast::Sender::send
    /// is also synchronous
    fn receive(&self, payload: Payload) -> Result<usize, broadcast::error::SendError<Payload>> {
        self.tx.send(payload)
    }

    /// Start a subscription. Fires a request to the upstream GraphQL.
    fn start<T: GraphQLQuery + Send + Sync>(
        &self,
        request_body: &graphql_client::QueryBody<T::Variables>,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<Payload>> {
        self.client_tx
            .send(Payload::start::<T>(self.id, request_body))
    }

    /// Stop a subscription. This has no return value since it'll be typically used by
    /// the `Drop` implementation.
    fn stop(&self) -> Result<(), tokio::sync::mpsc::error::SendError<Payload>> {
        self.client_tx.send(Payload::stop(self.id))
    }
}

impl Drop for Subscription {
    /// Send a close message upstream once the subscription drops out of scope.
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl<T: GraphQLQuery + Send + Sync> Receiver<T> for Subscription {
    /// Returns a stream of `Payload` responses, received from the GraphQL server.
    fn stream(&self) -> StreamResponse<T> {
        Box::pin(
            BroadcastStream::new(self.tx.subscribe())
                .filter(Result::is_ok)
                .map(|p| p.unwrap().response::<T>()),
        )
    }
}

/// A single `SubscriptionClient` enables subscription multiplexing.
#[derive(Debug)]
pub struct SubscriptionClient {
    tx: mpsc::UnboundedSender<Payload>,
    subscriptions: Arc<Mutex<WeakValueHashMap<Uuid, Weak<Subscription>>>>,
    _shutdown_tx: oneshot::Sender<()>,
}

impl SubscriptionClient {
    /// Create a new subscription client. `tx` is a channel for sending `Payload`s to the
    /// GraphQL server; `rx` is a channel for `Payload` back.
    fn new(tx: mpsc::UnboundedSender<Payload>, mut rx: mpsc::UnboundedReceiver<Payload>) -> Self {
        // Oneshot channel for cancelling the listener if SubscriptionClient is dropped
        let (_shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let subscriptions = Arc::new(Mutex::new(WeakValueHashMap::new()));
        let subscriptions_clone = Arc::clone(&subscriptions);

        // Spawn a handler for shutdown, and relaying received `Payload`s back to the relevant
        // subscription.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Break the loop if shutdown is triggered. This happens implicitly once
                    // the client goes out of scope
                    _ = &mut shutdown_rx => break,

                    // Handle receiving payloads back _from_ the server
                    Some(p) = rx.recv() => {
                        let s = subscriptions_clone.lock().unwrap().get::<Uuid>(&p.id);
                        if let Some(s) = s
                            as Option<Arc<Subscription>>
                        {
                            let _ = s.receive(p);
                        }
                    }
                }
            }
        });

        Self {
            tx,
            subscriptions,
            _shutdown_tx,
        }
    }

    /// Start a new subscription request.
    pub fn start<T: GraphQLQuery + Send + Sync>(
        &self,
        request_body: &graphql_client::QueryBody<T::Variables>,
    ) -> BoxedSubscription<T>
    where
        T: GraphQLQuery + Send + Sync,
        <T as GraphQLQuery>::ResponseData: Unpin + Send + Sync + 'static,
    {
        // Generate a unique ID for the subscription. Subscriptions can be multiplexed
        // over a single connection, so we'll keep a copy of this against the client to
        // handling routing responses back to the relevant subscriber.
        let id = Uuid::new_v4();

        // Create a new subscription wrapper, mapped to the new ID and with a clone of the
        // tx channel to send payloads back upstream.
        let subscription = Arc::new(Subscription::new(id, self.tx.clone()));

        // Store the subscription in the WeakValueHashMap. This is converted internally into
        // a weak reference, to prevent dropped subscriptions lingering in memory.
        self.subscriptions
            .lock()
            .unwrap()
            .insert(id, Arc::clone(&subscription));

        // Initialize the connection with the relevant control messages.
        let _ = subscription.init();
        let _ = subscription.start::<T>(request_body);

        // The caller gets back a Box<dyn Receiver<T>>, to consume subscription payloads.
        Box::new(Arc::clone(&subscription) as Arc<dyn Receiver<T> + Send + Sync>)
    }
}

/// Connect to a new WebSocket GraphQL server endpoint, and return a `SubscriptionClient`.
/// This method will a) connect to a ws(s):// endpoint, and perform the initial handshake, and b)
/// set up channel forwarding to expose just the returned `Payload`s to the client.
pub async fn connect_subscription_client(
    url: Url,
) -> Result<SubscriptionClient, tokio_tungstenite::tungstenite::Error> {
    let (ws, _) = connect_async(url).await?;
    let (mut ws_tx, mut ws_rx) = futures::StreamExt::split(ws);

    let (send_tx, mut send_rx) = mpsc::unbounded_channel::<Payload>();
    let (recv_tx, recv_rx) = mpsc::unbounded_channel::<Payload>();

    // Forwarded received messages back upstream to the GraphQL server
    tokio::spawn(async move {
        while let Some(p) = send_rx.recv().await {
            let _ = ws_tx
                .send(Message::Text(serde_json::to_string(&p).unwrap()))
                .await;
        }
    });

    // Forward received messages to the receiver channel.
    tokio::spawn(async move {
        while let Some(Ok(Message::Text(m))) = ws_rx.next().await {
            if let Ok(p) = serde_json::from_str::<Payload>(&m) {
                let _ = recv_tx.send(p);
            }
        }
    });

    Ok(SubscriptionClient::new(send_tx, recv_rx))
}
