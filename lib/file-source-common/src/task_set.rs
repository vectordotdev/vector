use std::collections::HashMap;

use tokio::task::{Id, JoinError, JoinSet};

/// A set of tasks whose keys remain available even if a task panics or is cancelled.
pub struct TaskSet<K, T> {
    keys: HashMap<Id, K>,
    tasks: JoinSet<T>,
}

impl<K, T: 'static> Default for TaskSet<K, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, T: 'static> TaskSet<K, T> {
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
            tasks: JoinSet::new(),
        }
    }

    #[track_caller]
    pub fn spawn<F>(&mut self, key: K, task: F)
    where
        F: Future<Output = T> + Send + 'static,
        T: Send,
    {
        let handle = self.tasks.spawn(task);
        self.keys.insert(handle.id(), key);
    }

    pub async fn join_next(&mut self) -> Option<(K, Result<T, JoinError>)> {
        let (id, result) = match self.tasks.join_next_with_id().await? {
            Ok((id, value)) => (id, Ok(value)),
            Err(error) => (error.id(), Err(error)),
        };
        let key = self.keys.remove(&id).expect("task id missing from key map");
        Some((key, result))
    }
}

#[cfg(test)]
mod tests {
    use super::TaskSet;
    use std::{io, sync::Arc};

    #[tokio::test]
    async fn completed_tasks_return_their_keys_and_release_them() {
        let mut set = TaskSet::new();
        let success = Arc::new("success");
        let failure = Arc::new("failure");
        set.spawn(Arc::clone(&success), async { Ok(42) });
        set.spawn(Arc::clone(&failure), async {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });

        // Completion order is unspecified; each result must retain its own key.
        for _ in 0..2 {
            let (key, result) = set.join_next().await.unwrap();
            match *key {
                "success" => assert_eq!(result.unwrap().unwrap(), 42),
                "failure" => assert_eq!(
                    result.unwrap().unwrap_err().kind(),
                    io::ErrorKind::PermissionDenied
                ),
                _ => unreachable!(),
            }
        }
        assert!(set.join_next().await.is_none());
        assert_eq!(Arc::strong_count(&success), 1);
        assert_eq!(Arc::strong_count(&failure), 1);
    }

    #[tokio::test]
    async fn panicked_task_returns_its_key_and_set_remains_usable() {
        let mut set = TaskSet::new();
        set.spawn("panicked", async { panic!("task failed") });
        let (key, result) = set.join_next().await.unwrap();
        assert_eq!(key, "panicked");
        assert!(result.unwrap_err().is_panic());
        assert!(set.join_next().await.is_none());

        set.spawn("next", async {});
        let (key, result) = set.join_next().await.unwrap();
        assert_eq!(key, "next");
        result.unwrap();
        assert!(set.join_next().await.is_none());
    }

    #[tokio::test]
    async fn cancelled_task_returns_its_key() {
        let mut set = TaskSet::new();
        set.spawn("cancelled", std::future::pending::<()>());
        set.tasks.abort_all();
        let (key, result) = set.join_next().await.unwrap();
        assert_eq!(key, "cancelled");
        assert!(result.unwrap_err().is_cancelled());
        assert!(set.join_next().await.is_none());
    }
}
