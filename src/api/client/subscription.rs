use futures::{SinkExt, StreamExt};
use futures_util;
use graphql_client::GraphQLQuery;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Weak;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc::error::SendError;
use tokio::{
    net::TcpStream,
    select,
    sync::{mpsc, oneshot},
};
use tokio_tungstenite::tungstenite::{Error, Message};
use tokio_tungstenite::{connect_async, tungstenite, WebSocketStream};
use uuid::Uuid;
use weak_table::WeakValueHashMap;

#[derive(Serialize, Deserialize, Debug)]
struct Payload {
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

pub struct Subscription {
    id: Uuid,
    tx: RwLock<mpsc::Sender<Payload>>,
    rx: mpsc::Receiver<Payload>,
}

impl Subscription {
    pub fn new(id: Uuid) -> Self {
        let (tx, rx) = mpsc::channel(10);

        Self {
            id,
            tx: RwLock::new(tx),
            rx,
        }
    }

    pub async fn send(&self, payload: Payload) -> Result<(), SendError<Payload>> {
        self.tx.write().unwrap().send(payload).await
    }

    pub fn receive(&self) -> &mpsc::Receiver<Payload> {
        &self.rx
    }

    // pub async fn close(&mut self) -> Result<(), SendError<Payload>> {
    //     self.tx.send(Payload::close(self.id)).await
    // }
}

pub struct SubscriptionClient {
    ws_tx: futures_util::stream::SplitSink<WebSocketStream<TcpStream>, Message>,
    subscriptions: Arc<RwLock<WeakValueHashMap<Uuid, Weak<Subscription>>>>,
}

impl SubscriptionClient {
    fn new(tx: WebSocketStream<TcpStream>) -> Self {
        // Oneshot channel for cancelling the listener if SubscriptionClient is dropped
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        // Create a hashmap to store subscriptions. This needs to be thread safe and behind
        // a RWLock, to handle looking up by subscription ID when receiving 'global' payloads.
        let subscriptions = Arc::new(RwLock::new(WeakValueHashMap::new()));

        // Split the receiver channel
        let (ws_tx, mut ws_rx) = tx.split();

        // let res = ws_rx.next().await;
        //
        // let payload: Option<Payload> = res
        //     .and_then(|r| r.ok())
        //     .and_then(|r| r.to_text().ok().and_then(|t| serde_json::from_str(t).ok()));
        //
        // if let Some(p) = payload {
        //     if let Some(mut s) = subscriptions.read().unwrap().get::<Uuid>(&p.id) {
        //         s.tx.send(p).await;
        //     }
        // }

        // Spawn a handler for receiving payloads back from the client.
        tokio::spawn(async move {
            loop {
                select! {
                    _ = &mut shutdown_rx => break,
                    res = &mut ws_rx.next() => {
                        let payload: Option<Payload> = res
                            .and_then(|r| r.ok())
                            .and_then(|r| r.to_text().ok().and_then(|t| serde_json::from_str(t).ok()));

                        if let Some(p) = payload {
                            if let Some(s) = subscriptions.read().unwrap().get::<Uuid>(&p.id) {
                                Subscription::send(s, p).await;
                            }
                        }
                    }
                }
            }
        });

        // Return a new client
        Self {
            ws_tx,
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
