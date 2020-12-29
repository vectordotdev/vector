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
pub async fn query<T, I: ExactSizeIterator<Item = T>>(
    iter: I,
    p: Params,
    default_page_size: usize,
) -> ConnectionResult<T> {
    connection::query::<usize, T, _, _, _, _>(
        p.after,
        p.before,
        p.first,
        p.last,
        |after, before, first, last| async move {
            let iter_len = iter.len();

            let (start, end) = {
                let after = after.map(|after| after + 1).unwrap_or(0);
                let before = before.unwrap_or(iter_len);

                if after > before {
                    (0, 0)
                } else {
                    match (first, last) {
                        // First
                        (Some(first), _) => (after, (after + first).min(before)),
                        // Last
                        (_, Some(last)) => {
                            ((before.checked_sub(last)).unwrap_or(0).max(after), before)
                        }
                        // Default page size
                        _ => (after, default_page_size.min(before)),
                    }
                }
            };

            let mut connection = Connection::new(start > 0, end < iter_len);
            connection.append(
                (start..end)
                    .into_iter()
                    .zip(iter.skip(start))
                    .map(|(cursor, node)| Edge::new(cursor, node)),
            );
            Ok(connection)
        },
    )
    .await
}
