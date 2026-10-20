//! Per-connection metadata attached by a [`ClientCertVerifier`](super::ClientCertVerifier).

use std::sync::{Arc, LazyLock, OnceLock};

use lookup::path;
use openssl::{
    ex_data::Index,
    ssl::{Ssl, SslRef},
};
use vrl::value::{ObjectMap, Value};

use crate::event::Event;

/// The event metadata field, under the source's name, that holds a connection's
/// [`TlsPeerMetadata`].
pub const TLS_CLIENT_IDENTITY_KEY: &str = "tls_client_identity";

/// The metadata a [`ClientCertVerifier`](super::ClientCertVerifier) attached to one accepted TLS
/// connection.
///
/// The handle exists as soon as the connection is accepted and is filled once the handshake
/// completes, so servers that set up per-connection state before the handshake (as HTTP and gRPC
/// servers do) can capture it early and read it per request.
#[derive(Clone, Debug, Default)]
pub struct TlsPeerMetadata(pub(super) Arc<OnceLock<ObjectMap>>);

impl TlsPeerMetadata {
    /// The metadata, once the handshake completed and if the verifier attached any.
    pub fn get(&self) -> Option<&ObjectMap> {
        self.0.get()
    }

    /// Writes the metadata, if any, into each event's metadata at
    /// `%<source_name>.tls_client_identity`. Event metadata is used in both log namespaces, so the
    /// value never mixes with client-supplied event fields.
    pub fn insert_into(&self, events: &mut [Event], source_name: &str) {
        let Some(metadata) = self.get() else {
            return;
        };
        for event in events {
            event.metadata_mut().value_mut().insert(
                path!(source_name, TLS_CLIENT_IDENTITY_KEY),
                Value::Object(metadata.clone()),
            );
        }
    }

    /// Installs this handle on a connection before its handshake, so the verify callback can fill
    /// it.
    pub(super) fn attach(&self, ssl: &mut SslRef) {
        if let Some(index) = *INDEX {
            ssl.set_ex_data(index, self.clone());
        }
    }

    /// Fills the handle installed on `ssl`, if any.
    pub(super) fn fill(ssl: &SslRef, metadata: ObjectMap) {
        if let Some(handle) = INDEX.and_then(|index| ssl.ex_data(index)) {
            // A handshake verifies the leaf once, so the slot is only ever set once.
            handle.0.set(metadata).ok();
        }
    }
}

/// OpenSSL ex-data slot holding each connection's handle. Allocating it only fails when OpenSSL is
/// out of memory; connections then carry no metadata, but verification itself is unaffected.
static INDEX: LazyLock<Option<Index<Ssl, TlsPeerMetadata>>> =
    LazyLock::new(|| Ssl::new_ex_index().ok());
