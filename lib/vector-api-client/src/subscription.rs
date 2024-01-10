use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex},
};

use futures::SinkExt;
use graphql_client::GraphQLQuery;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{
    broadcast::{self, Sender},
    mpsc, oneshot,
};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;
use uuid::Uuid;

/// Subscription GraphQL response, returned from an active stream.
pub type BoxedSubscription<T> = Pin<
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

/// A single `SubscriptionClient` enables subscription multiplexing.
#[derive(Debug)]
pub struct SubscriptionClient {
    tx: mpsc::UnboundedSender<Payload>,
    subscriptions: Arc<Mutex<HashMap<Uuid, Sender<Payload>>>>,
    _shutdown_tx: oneshot::Sender<()>,
}

impl SubscriptionClient {
    /// Create a new subscription client. `tx` is a channel for sending `Payload`s to the
    /// GraphQL server; `rx` is a channel for `Payload` back.
    fn new(tx: mpsc::UnboundedSender<Payload>, mut rx: mpsc::UnboundedReceiver<Payload>) -> Self {
        // Oneshot channel for cancelling the listener if SubscriptionClient is dropped
        let (_shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let subscriptions = Arc::new(Mutex::new(HashMap::new()));
        let subscriptions_clone = Arc::clone(&subscriptions);

        // Spawn a handler for shutdown, and relaying received `Payload`s back to the relevant
        // subscription.
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Break the loop if shutdown is triggered. This happens implicitly once
                    // the client goes out of scope
                    _ = &mut shutdown_rx => {
                        let subscriptions = subscriptions_clone.lock().unwrap();
                        for id in subscriptions.keys() {
                            _ = tx_clone.send(Payload::stop(*id));
                        }
                        break
                    },

                    // Handle receiving payloads back _from_ the server
                    message = rx.recv() => {
                        match message {
                            Some(p) => {
                                let subscriptions = subscriptions_clone.lock().unwrap();
                                let s: Option<&Sender<Payload>> = subscriptions.get::<Uuid>(&p.id);
                                if let Some(s) = s {
                                    _ = s.send(p);
                                }
                            }
                            None => {
                                subscriptions_clone.lock().unwrap().clear();
                                break;
                            },
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

        let (tx, rx) = broadcast::channel::<Payload>(100);

        self.subscriptions.lock().unwrap().insert(id, tx);

        // Initialize the connection with the relevant control messages.
        _ = self.tx.send(Payload::init(id));
        _ = self.tx.send(Payload::start::<T>(id, request_body));

        Box::pin(
            BroadcastStream::new(rx)
                .filter(Result::is_ok)
                .map(|p| p.unwrap().response::<T>()),
        )
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
            _ = ws_tx
                .send(Message::Text(serde_json::to_string(&p).unwrap()))
                .await;
        }
    });

    // Forward received messages to the receiver channel.
    tokio::spawn(async move {
        while let Some(Ok(Message::Text(m))) = ws_rx.next().await {
            if let Ok(p) = serde_json::from_str::<Payload>(&m) {
                _ = recv_tx.send(p);
            }
        }
    });

    Ok(SubscriptionClient::new(send_tx, recv_rx))
}
