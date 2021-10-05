mod common;

use buffers::{Variant, WhenFull};
use common::Message;
use futures::{SinkExt, StreamExt};
use rand::Rng;
use std::mem;
use tokio::runtime;

struct TempDir {
    inner: std::path::PathBuf,
}

impl TempDir {
    fn new(id: String) -> Self {
        let mut rng = rand::thread_rng();
        let mut dir = std::path::PathBuf::new();
        dir.push(std::env::temp_dir());
        dir.push(id);
        dir.push(rng.gen::<u64>().to_string());

        std::fs::create_dir_all(&dir).unwrap();
        Self { inner: dir }
    }

    fn path_buf(&self) -> std::path::PathBuf {
        self.inner.clone()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.inner).unwrap()
    }
}

// Assert that all messages are transmitted through the buffer in
// order. Depending on the variant all messages might not be transmitted but
// those that are must have their ordering preserved.
#[test]
fn in_order() {
    loom::model(|| {
        let id = "buffer-in_order".to_string();
        let tempdir = TempDir::new(id.clone());
        let variant = Variant::Disk {
            max_size: mem::size_of::<Message>() * 10,
            when_full: WhenFull::Block,
            data_dir: tempdir.path_buf(),
            id,
        };
        let mut messages = Vec::with_capacity(128);
        for i in 0..128 {
            messages.push(Message::new(i));
        }

        let runtime = runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .unwrap();

        let (bic, mut rx, _) = buffers::build::<Message>(variant).unwrap();
        let mut tx = bic.get();
        drop(bic);

        runtime.block_on(async move {
            let _ = tokio::spawn(async move {
                for message in messages.into_iter() {
                    tx.send(message).await.unwrap();
                }
            });

            let mut prev = None;
            while let cur @ Some(_) = rx.next().await {
                assert!(common::are_in_order(&prev, &cur));
                prev = cur;
            }
        });
    });
}

// // Assert that all messages are transmitted through the buffer with no
// // message loss if the variant blocks when full.
// #[test]
// fn no_loss() {
//     loom::model(|| {
//         let variant = Variant::Memory {
//             max_events: 10,
//             when_full: WhenFull::Block,
//         };
//         let mut messages = Vec::with_capacity(128);
//         for i in 0..128 {
//             messages.push(Message::new(i));
//         }

//         let expected_total = messages.len();
//         let runtime = runtime::Builder::new_multi_thread()
//             .worker_threads(2)
//             .build()
//             .unwrap();

//         let (bic, mut rx, acker) = buffers::build::<Message>(variant).unwrap();
//         let mut tx = bic.get();
//         drop(bic);

//         runtime.block_on(async move {
//             let _ = tokio::spawn(async move {
//                 for message in messages.into_iter() {
//                     tx.send(message).await.unwrap();
//                 }
//             });

//             let mut actual_total = 0;
//             while let Some(_) = rx.next().await {
//                 acker.ack(1);
//                 actual_total += 1;
//             }
//             assert_eq!(expected_total, actual_total);
//         });
//     });
// }
