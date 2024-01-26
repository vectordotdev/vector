use std::convert::Infallible;

use async_graphql::{
    connection::{self, Connection, CursorType, Edge, EmptyFields},
    Result, SimpleObject,
};
use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};

/// Base64 invalid states, used by `Base64Cursor`.
pub enum Base64CursorError {
    /// Invalid cursor. This can happen if the base64 string is valid, but its contents don't
    /// conform to the `name:index` pattern.
    Invalid,
    /// Decoding error. If this happens, the string isn't valid base64.
    DecodeError(base64::DecodeError),
}

impl std::fmt::Display for Base64CursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid cursor")
    }
}

/// Base64 cursor implementation
pub struct Base64Cursor {
    name: &'static str,
    index: usize,
}

impl Base64Cursor {
    const fn new(index: usize) -> Self {
        Self {
            name: "Cursor",
            index,
        }
    }

    /// Returns a base64 string representation of the cursor
    fn encode(&self) -> String {
        BASE64_URL_SAFE_NO_PAD.encode(format!("{}:{}", self.name, self.index))
    }

    /// Decodes a base64 string into a cursor result
    fn decode(s: &str) -> Result<Self, Base64CursorError> {
        let bytes = BASE64_URL_SAFE_NO_PAD
            .decode(s)
            .map_err(Base64CursorError::DecodeError)?;

        let cursor = String::from_utf8(bytes).map_err(|_| Base64CursorError::Invalid)?;
        let index = cursor
            .split(':')
            .last()
            .map(|s| s.parse::<usize>())
            .ok_or(Base64CursorError::Invalid)?
            .map_err(|_| Base64CursorError::Invalid)?;

        Ok(Self::new(index))
    }

    /// Increment and return the index. Uses saturating_add to avoid overflow
    /// issues.
    const fn increment(&self) -> usize {
        self.index.saturating_add(1)
    }
}

impl From<Base64Cursor> for usize {
    fn from(cursor: Base64Cursor) -> Self {
        cursor.index
    }
}

/// Makes the `Base64Cursor` compatible with Relay connections
impl CursorType for Base64Cursor {
    type Error = Base64CursorError;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        Base64Cursor::decode(s)
    }

    fn encode_cursor(&self) -> String {
        self.encode()
    }
}

/// Additional fields to attach to the connection
#[derive(SimpleObject)]
pub struct ConnectionFields {
    /// Total result set count
    total_count: usize,
}

/// Relay connection result
pub type ConnectionResult<T> = Result<Connection<Base64Cursor, T, ConnectionFields, EmptyFields>>;

/// Relay-compliant connection parameters to page results by cursor/page size
pub struct Params {
    after: Option<String>,
    before: Option<String>,
    first: Option<i32>,
    last: Option<i32>,
}

impl Params {
    pub const fn new(
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> Self {
        Self {
            after,
            before,
            first,
            last,
        }
    }
}

/// Creates a new Relay-compliant connection. Iterator must implement `ExactSizeIterator` to
/// determine page position in the total result set.
pub async fn query<T: async_graphql::OutputType, I: ExactSizeIterator<Item = T>>(
    iter: I,
    p: Params,
    default_page_size: usize,
) -> ConnectionResult<T> {
    connection::query::<_, _, Base64Cursor, _, _, ConnectionFields, _, _, _, Infallible>(
        p.after,
        p.before,
        p.first,
        p.last,
        |after, before, first, last| async move {
            let iter_len = iter.len();

            let (start, end) = {
                let after = after.map(|a| a.increment()).unwrap_or(0);
                let before: usize = before.map(|b| b.into()).unwrap_or(iter_len);

                // Calculate start/end based on the provided first/last. Note that async-graphql disallows
                // providing both (returning an error), so we can safely assume we have, at most, one of
                // first or last.
                match (first, last) {
                    // First
                    (Some(first), _) => (after, (after.saturating_add(first)).min(before)),
                    // Last
                    (_, Some(last)) => ((before.saturating_sub(last)).max(after), before),
                    // Default page size
                    _ => (after, default_page_size.min(before)),
                }
            };

            let mut connection = Connection::with_additional_fields(
                start > 0,
                end < iter_len,
                ConnectionFields {
                    total_count: iter_len,
                },
            );
            connection.edges.extend(
                (start..end)
                    .zip(iter.skip(start))
                    .map(|(cursor, node)| Edge::new(Base64Cursor::new(cursor), node)),
            );
            Ok(connection)
        },
    )
    .await
}
