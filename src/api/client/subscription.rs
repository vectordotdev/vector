use futures::{SinkExt, Stream};
use futures_util::stream::SplitSink;
use graphql_client::GraphQLQuery;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    boxed::Box,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, RwLock, Weak},
};
use tokio::{net::TcpStream, select, stream::StreamExt, sync::broadcast, sync::oneshot};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Error, Message},
    WebSocketStream,
};
use uuid::Uuid;
use weak_table::WeakValueHashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Payload {
    id: Uuid,
    #[serde(rename = "type")]
    payload_type: String,
}

impl Payload {
    fn close(id: Uuid) -> Self {
        Self {
            id,
            payload_type: String::from("close"),
        }
    }
}

pub struct Subscription<T: GraphQLQuery> {
    id: Uuid,
    tx: broadcast::Sender<Payload>,
    payload: GraphQLQuer,
}

impl Subscription {
    /// Returns a new Subscription, with the broadcast::Sender stored against it. Here,
    /// we are ignoring the broadcast::Receiver, because it can be cloned with
    /// Sender.subscribe()
    pub fn new(id: Uuid) -> Self {
        let (tx, _) = broadcast::channel(1);
        Self { id, tx }
    }

    /// Send a payload down the channel. This is synchronous because broadcast::Sender::send
    /// is also synchronous
    fn transmit(&self, payload: Payload) -> Result<usize, broadcast::SendError<Payload>> {
        self.tx.send(payload)
    }

    /// Returns a stream of `Payload` responses, received from the GraphQL server
    pub fn stream(&self) -> Pin<Box<impl Stream<Item = Payload>>> {
        Box::pin(
            self.tx
                .subscribe()
                .into_stream()
                .filter(Result::is_ok)
                .map(Result::unwrap),
        )
    }
}

pub struct SubscriptionClient {
    ws_tx: SplitSink<WebSocketStream<TcpStream>, Message>,
    subscriptions: Arc<RwLock<WeakValueHashMap<Uuid, Weak<Subscription>>>>,
    _shutdown_tx: oneshot::Sender<()>,
}

impl SubscriptionClient {
    fn new(tx: WebSocketStream<TcpStream>) -> Self {
        // Oneshot channel for cancelling the listener if SubscriptionClient is dropped
        let (_shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        // Create a hashmap to store subscriptions. This needs to be thread safe and behind
        // a RWLock, to handle looking up by subscription ID when receiving 'global' payloads.
        let subscriptions = Arc::new(RwLock::new(WeakValueHashMap::new()));

        // Split the receiver channel
        let (ws_tx, mut ws_rx) = futures::StreamExt::split(tx);

        // Spawn a handler for receiving payloads back from the client.
        let spawned_subscriptions = Arc::clone(&subscriptions);
        tokio::spawn(async move {
            loop {
                select! {
                    // Break the loop if shutdown is triggered. This happens implicitly once
                    // the client goes out of scope
                    _ = &mut shutdown_rx => break,

                    // Handle received payloads back from the server
                    res = &mut ws_rx.next() => {

                        // Attempt to both deserialize the payload, and obtain a subscription
                        // with a matching ID. Rust cannot infer the Arc type, so being explicit here
                        let sp: Option<(Option<Arc<Subscription>>, Payload)> = res
                            .and_then(|r| r.ok())
                            .and_then(|r| {
                                r.to_text()
                                    .ok()
                                    .and_then(|t| serde_json::from_str::<Payload>(t).ok())
                            }).and_then(|p| Some((spawned_subscriptions.read().unwrap().get::<Uuid>(&p.id), p)));

                        if let Some((Some(s), p)) = sp {
                            let _ = s.transmit(p);
                        }
                    }
                }
            }
        });

        // Return a new client
        Self {
            ws_tx,
            _shutdown_tx,
            subscriptions: Arc::clone(&subscriptions),
        }
    }

    /// Start a new subscription request
    pub async fn start<T: GraphQLQuery>(
        &mut self,
        request_body: &graphql_client::QueryBody<T::Variables>,
    ) -> Result<Arc<Subscription>, Error> {
        // Generate a unique ID for the subscription. Subscriptions can be multiplexed
        // over a single connection, so we'll keep a copy of this against the client to
        // handling routing responses back to the relevant subscriber.
        let id = Uuid::new_v4();

        // Create a new subscription wrapper, which will contain its own tx/rx channels
        // to receive payloads
        let subscription = Arc::new(Subscription::new(id));

        // Store the subscription in the WeakValueHashMap. This is converted internally into
        // a weak reference, to prevent dropped subscriptions lingering in memory
        self.subscriptions
            .write()
            .unwrap()
            .insert(id, Arc::clone(&subscription));

        // Augment the GraphQL request body with `id` and `type: start` controls
        let json = json!({
            "id": id,
            "type": "start",
            "payload": request_body
        });

        // Send the message over the currently connected socket
        self.ws_tx
            .send(Message::Text(json.to_string().into()))
            .await?;

        Ok(subscription)
    }
}

/// Connect to a GraphQL subscription endpoint and return an active client. Can be used for
/// multiple subscriptions
pub async fn make_subscription_client(
    addr: SocketAddr,
) -> Result<SubscriptionClient, tungstenite::error::Error> {
    let url = &*format!("ws://{}:{}/graphql", addr.ip(), addr.port());
    let (tx, _) = connect_async(url).await?;
    let client = SubscriptionClient::new(tx);

    Ok(client)
}
