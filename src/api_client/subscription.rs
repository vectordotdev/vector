use futures::{SinkExt, Stream};
use graphql_client::GraphQLQuery;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    boxed::Box,
    pin::Pin,
    sync::{Arc, RwLock, Weak},
};
use tokio::{
    net::TcpStream,
    select,
    stream::StreamExt,
    sync::{
        broadcast::{self, SendError},
        oneshot,
    },
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Error, Message},
    WebSocketStream,
};
use url::Url;
use uuid::Uuid;
use weak_table::WeakValueHashMap;

// Payload contains the raw data received back from a GraphQL subscription. At the point
// of receiving data, the only known fields are { id, type }; what's contained inside the
// `payload` field is unknown until we attempt to deserialize it against a generated
// GraphQLQuery::ResponseData later
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Payload {
    id: Uuid,
    #[serde(rename = "type")]
    payload_type: String,
    payload: serde_json::Value,
}

impl Payload {
    /// Returns a "start" payload necessary for starting a new subscription
    fn start<T: GraphQLQuery>(id: Uuid, payload: &graphql_client::QueryBody<T::Variables>) -> Self {
        Self {
            id,
            payload_type: "start".to_owned(),
            payload: json!(payload),
        }
    }

    /// Returns a "stop" payload for terminating the subscription in the GraphQL server
    fn stop(id: Uuid) -> Self {
        Self {
            id,
            payload_type: "stop".to_owned(),
            payload: serde_json::Value::Null,
        }
    }

    /// Attempts to return a definitive ResponseData on the `payload` field, matched against
    /// a generated GraphQLQuery
    fn response<T: GraphQLQuery>(&self) -> Option<graphql_client::Response<T::ResponseData>> {
        serde_json::from_value::<graphql_client::Response<T::ResponseData>>(self.payload.clone())
            .ok()
    }
}
pub trait Receiver<T: GraphQLQuery> {
    fn stream(
        &self,
    ) -> Pin<Box<dyn Stream<Item = Option<graphql_client::Response<T::ResponseData>>>>>;
}

#[derive(Debug)]
pub struct Subscription {
    id: Uuid,
    tx: broadcast::Sender<Payload>,
    client_tx: broadcast::Sender<Payload>,
}

impl Subscription {
    pub fn new(id: Uuid, client_tx: broadcast::Sender<Payload>) -> Self {
        let (tx, _) = broadcast::channel(1);
        Self { id, tx, client_tx }
    }

    /// Send a payload down the channel. This is synchronous because broadcast::Sender::send
    /// is also synchronous
    fn transmit(&self, payload: Payload) -> Result<usize, SendError<Payload>> {
        self.tx.send(payload)
    }

    /// Stop a subscription. This has no return value since it'll be typically used by
    /// the Drop implementation
    fn stop(&self) -> Result<usize, SendError<Payload>> {
        self.tx.send(Payload::stop(self.id))
    }
}

impl Drop for Subscription {
    /// Send a close message upstream once the subscription drops out of scope
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl<T: GraphQLQuery> Receiver<T> for Subscription {
    /// Returns a stream of `Payload` responses, received from the GraphQL server
    fn stream(
        &self,
    ) -> Pin<Box<dyn Stream<Item = Option<graphql_client::Response<T::ResponseData>>>>> {
        Box::pin(
            self.tx
                .subscribe()
                .into_stream()
                .filter(Result::is_ok)
                .map(Result::unwrap)
                .map(|p| p.response::<T>()),
        )
    }
}

pub struct SubscriptionClient {
    tx: broadcast::Sender<Payload>,
    subscriptions: Arc<RwLock<WeakValueHashMap<Uuid, Weak<Subscription>>>>,
    _shutdown_tx: oneshot::Sender<()>,
}

impl SubscriptionClient {
    fn new(ws: WebSocketStream<TcpStream>) -> Self {
        // Oneshot channel for cancelling the listener if SubscriptionClient is dropped
        let (_shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        // Create a hashmap to store subscriptions. This needs to be thread safe and behind
        // a RWLock, to handle looking up by subscription ID when receiving 'global' payloads.
        let subscriptions = Arc::new(RwLock::new(WeakValueHashMap::new()));

        // Split the WebSocket channels
        let (mut ws_tx, mut ws_rx) = futures::StreamExt::split(ws);

        // Create a multi producer channel for sending messages back upstream
        let (tx, mut rx) = broadcast::channel::<Payload>(100);

        // Spawn a handler for receiving payloads back from the client.
        let spawned_subscriptions = Arc::clone(&subscriptions);
        tokio::spawn(async move {
            loop {
                select! {
                    // Break the loop if shutdown is triggered. This happens implicitly once
                    // the client goes out of scope
                    _ = &mut shutdown_rx => break,

                    // Handle received payloads back _from_ the server
                    res = &mut ws_rx.next() => {
                        // Attempt to both deserialize the payload, and obtain a subscription
                        // with a matching ID. Rust cannot infer the Arc type, so being explicit here
                        let sp: Option<(Option<Arc<Subscription>>, Payload)> = res
                            .and_then(|r| r.ok())
                            .and_then(|r| {
                                r.to_text()
                                    .ok()
                                    .and_then(|t| serde_json::from_str::<Payload>(t).ok())
                            }).map(|p| (spawned_subscriptions.read().unwrap().get::<Uuid>(&p.id), p));

                        // If we have a payload and a valid Subscription that matches the returned
                        // id, send the payload into the subscription to be picked up by its .stream()
                        if let Some((Some(s), p)) = sp {
                            let _ = s.transmit(p);
                        }
                    },

                    // Handle payloads to be sent _to_ the GraphQL server
                    payload = &mut rx.next() => {
                        if let Some(p) = payload.and_then(|p| p.ok()) {
                            let _ = ws_tx.send(Message::Text(serde_json::to_string(&p).unwrap())).await;
                        }
                    },
                }
            }
        });

        // Return a new client
        Self {
            tx,
            _shutdown_tx,
            subscriptions: Arc::clone(&subscriptions),
        }
    }

    /// Start a new subscription request
    pub async fn start<T: GraphQLQuery>(
        &self,
        request_body: &graphql_client::QueryBody<T::Variables>,
    ) -> Result<Box<Arc<dyn Receiver<T>>>, Error> {
        // Generate a unique ID for the subscription. Subscriptions can be multiplexed
        // over a single connection, so we'll keep a copy of this against the client to
        // handling routing responses back to the relevant subscriber.
        let id = Uuid::new_v4();

        // Create a new subscription wrapper, mapped to the new ID and with a clone of the
        // tx channel to send payloads back upstream
        let subscription = Arc::new(Subscription::new(id, self.tx.clone()));

        // Store the subscription in the WeakValueHashMap. This is converted internally into
        // a weak reference, to prevent dropped subscriptions lingering in memory
        self.subscriptions
            .write()
            .unwrap()
            .insert(id, Arc::clone(&subscription));

        // Start the subscription by sending a { type: "start" } payload upstream
        let _ = self.tx.send(Payload::start::<T>(id, request_body));

        // The caller gets back a Box<dyn Receiver<T>>, to consume subscription payloads
        Ok(Box::new(Arc::clone(&subscription) as Arc<dyn Receiver<T>>))
    }
}

/// Connect to a GraphQL subscription endpoint and return an active client. Can be used for
/// multiple subscriptions
pub async fn make_subscription_client(
    url: &Url,
) -> Result<SubscriptionClient, tungstenite::error::Error> {
    let (tx, _) = connect_async(url).await?;
    let client = SubscriptionClient::new(tx);

    Ok(client)
}
