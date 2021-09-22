use crate::test::common::Message;
use crate::Variant;
use crate::WhenFull;
use futures::{stream, StreamExt};
use proptest::prelude::*;
use tokio::runtime;

/// `VariantGuard` wraps a `Variant`, allowing a convenient Drop implementation
struct VariantGuard {
    inner: Variant,
}

impl VariantGuard {
    fn new(variant: Variant) -> Self {
        VariantGuard { inner: variant }
    }
}

impl AsRef<Variant> for VariantGuard {
    fn as_ref(&self) -> &Variant {
        &self.inner
    }
}

impl Drop for VariantGuard {
    fn drop(&mut self) {
        match &self.inner {
            Variant::Memory { .. } => { /* nothing to clean up */ }
            #[cfg(feature = "disk-buffer")]
            Variant::Disk { data_dir, .. } => {
                // SAFETY: Here we clean up the data_dir of the inner `Variant`,
                // see note in the constructor for this type.
                std::fs::remove_dir_all(data_dir).unwrap();
            }
        }
    }
}

#[cfg(feature = "disk-buffer")]
fn arb_variant() -> impl Strategy<Value = Variant> {
    prop_oneof![
        <(u16, WhenFull)>::arbitrary().prop_map(|(max_events, when_full)| {
            Variant::Memory {
                max_events: max_events as usize,
                when_full,
            }
        }),
        <(u16, WhenFull, u64)>::arbitrary().prop_map(|(max_size, when_full, id)| {
            let id = id.to_string();
            // SAFETY: We allow tempdir to create the directory but by
            // calling `into_path` we obligate ourselves to delete it. This
            // is done in the drop implementation for `VariantGuard`.
            let data_dir = tempdir::TempDir::new_in(std::env::temp_dir(), &id)
                .unwrap()
                .into_path();
            Variant::Disk {
                max_size: max_size as usize,
                when_full,
                data_dir,
                id,
            }
        })
    ]
}

#[cfg(not(feature = "disk-buffer"))]
fn arb_variant() -> impl Strategy<Value = Variant> {
    prop_oneof![
        <(u16, WhenFull)>::arbitrary().prop_map(|(max_events, when_full)| {
            Variant::Memory {
                max_events: max_events as usize,
                when_full,
            }
        }),
    ]
}

#[cfg(feature = "disk-buffer")]
fn arb_variant_blocking() -> impl Strategy<Value = Variant> {
    prop_oneof![
        <u16>::arbitrary().prop_map(|max_events| {
            Variant::Memory {
                max_events: max_events as usize,
                when_full: WhenFull::Block,
            }
        }),
        <(u16, u64)>::arbitrary().prop_map(|(max_size, id)| {
            let id = id.to_string();
            // SAFETY: We allow tempdir to create the directory but by
            // calling `into_path` we obligate ourselves to delete it. This
            // is done in the drop implementation for `VariantGuard`.
            let data_dir = tempdir::TempDir::new_in(std::env::temp_dir(), &id)
                .unwrap()
                .into_path();
            Variant::Disk {
                max_size: max_size as usize,
                when_full: WhenFull::Block,
                data_dir,
                id,
            }
        })
    ]
}

#[cfg(not(feature = "disk-buffer"))]
fn arb_variant_blocking() -> impl Strategy<Value = Variant> {
    prop_oneof![<u16>::arbitrary().prop_map(|max_events| {
        Variant::Memory {
            max_events: max_events as usize,
            when_full: WhenFull::Block,
        }
    }),]
}

fn arb_messages() -> impl Strategy<Value = Vec<Message>> {
    <u8>::arbitrary().prop_map(|total_messages| {
        let total_messages = total_messages as usize;
        let mut messages = Vec::with_capacity(total_messages);
        for i in 0..total_messages {
            messages.push(Message::new(i as u64));
        }
        messages
    })
}

fn are_in_order<T>(prev: &Option<T>, cur: &Option<T>) -> bool
where
    T: PartialOrd,
{
    match (prev, cur) {
        (None, None) => true,
        (None, Some(_)) => true,
        (Some(_), None) => true,
        (Some(p), Some(c)) => p < c,
    }
}

proptest! {
    // Assert that all messages are transmitted through the buffer in
    // order. Depending on the variant all messages might not be transmitted but
    // those that are must have their ordering preserved.
    #[test]
    fn in_order(variant in arb_variant(), messages in arb_messages()) {
        let runtime = runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .unwrap();

        let guard = VariantGuard::new(variant);
        let (bic, mut rx, _) = crate::build::<Message>(guard.as_ref().clone()).unwrap();
        let tx = bic.get();
        drop(bic);

        runtime.block_on(async move {
            let _ = tokio::spawn(async move {
                let  stream = stream::iter(messages.clone()).map(|m| Ok(m));
                stream.forward(tx).await.unwrap();
            });

            let mut prev = None;
            while let cur @ Some(_) = rx.next().await {
                assert!(are_in_order(&prev, &cur));
                prev = cur;
            }
        })
    }
}

proptest! {
    // Assert that all messages are transmitted through the buffer with no
    // message loss if the variant blocks when full.
    #[test]
    fn no_loss(variant in arb_variant_blocking(), messages in arb_messages()) {
        let expected_total = messages.len();
        let runtime = runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .unwrap();

        let guard = VariantGuard::new(variant);
        let (bic, mut rx, _) = crate::build::<Message>(guard.as_ref().clone()).unwrap();
        let tx = bic.get();
        drop(bic);

        runtime.block_on(async move {
            let _ = tokio::spawn(async move {
                let  stream = stream::iter(messages.clone()).map(|m| Ok(m));
                stream.forward(tx).await.unwrap();
            });

            let mut actual_total = 0;
            while let Some(_) = rx.next().await {
                actual_total += 1;
            }
            assert_eq!(expected_total, actual_total);
        })
    }
}
