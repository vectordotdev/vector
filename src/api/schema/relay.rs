use async_graphql::{
    connection::{self, Connection, Edge, EmptyFields},
    Result,
};

/// Relay connection result
pub type ConnectionResult<T> = Result<Connection<usize, T, EmptyFields, EmptyFields>>;

/// Relay-compliant connection parameters to page results by cursor/page size
pub struct Params {
    after: Option<String>,
    before: Option<String>,
    first: Option<i32>,
    last: Option<i32>,
}

impl Params {
    pub fn new(
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
pub async fn query<T, I: ExactSizeIterator<Item = T>>(iter: I, p: Params) -> ConnectionResult<T> {
    connection::query(
        p.after,
        p.before,
        p.first,
        p.last,
        |after, before, first, last| async move {
            let iter_len = iter.len();
            let mut start = after.map(|after| after + 1).unwrap_or(0);

            // Calculate the end position based on the `before` cursor, and the number of desired
            // results.
            let mut end = before.unwrap_or(iter_len);
            if let Some(first) = first {
                end = (start + first).min(end);
            }
            if let Some(last) = last {
                start = if last > end - start { end } else { end - last };
            }

            let mut connection = Connection::new(start > 0, end < iter_len);
            connection.append(
                (start..end)
                    .into_iter()
                    .zip(iter.skip(start).take(end - start))
                    .map(|(cursor, node)| Edge::new(cursor, node)),
            );
            Ok(connection)
        },
    )
    .await
}
