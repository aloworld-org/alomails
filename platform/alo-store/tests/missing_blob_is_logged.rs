//! A message whose bytes are gone must say so in the log.
//!
//! The caller is deliberately told nothing: a body that cannot be read is the
//! same `NotFound` as a row that was never there, and on the wire that is
//! right — no internal detail, no oracle. It also means the log is the only
//! place the truth exists. When it was silent, a reading pane showing "could
//! not load messages" was indistinguishable from a database being down, and
//! the difference cost an afternoon.
//!
//! So the assertion here is on a log line, which is unusual and deliberate:
//! there is no other observable difference to assert on.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use alo_store::{BlobStore, StoreError};
use tracing::subscriber::with_default;
use tracing_subscriber::layer::SubscriberExt;

/// Collects formatted events so a test can read what was logged.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<String>>>);

impl Captured {
    fn contains(&self, needle: &str) -> bool {
        self.0.lock().unwrap().iter().any(|l| l.contains(needle))
    }

    fn dump(&self) -> String {
        self.0.lock().unwrap().join("\n")
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Captured {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        struct Visit(String);
        impl tracing::field::Visit for Visit {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                self.0.push_str(&format!(" {}={value:?}", field.name()));
            }
        }
        let mut visit = Visit(format!("{}", event.metadata().level()));
        event.record(&mut visit);
        self.0.lock().unwrap().push(visit.0);
    }
}

#[tokio::test]
async fn fetching_a_blob_that_is_not_there_says_which_object_is_missing() {
    let blobs = BlobStore::in_memory(1024 * 1024);
    let logged = Captured::default();
    let subscriber = tracing_subscriber::registry().with(logged.clone());

    // The hash of bytes that were never stored: exactly what a message row
    // points at once its object has been lost.
    let hash = "4ba152aae024c9626af7b36f2e163fc123a179094b06d9219e6958f08b7aeb1d";
    let result = with_default(subscriber, || {
        futures::executor::block_on(blobs.get("tenant-x", hash))
    });

    assert!(
        matches!(result, Err(StoreError::NotFound)),
        "the caller is told nothing more than NotFound",
    );
    assert!(
        logged.contains("missing from the blob store"),
        "but the log says what happened:\n{}",
        logged.dump(),
    );
    assert!(
        logged.contains(hash) && logged.contains("tenant-x"),
        "and names the object, which is what makes it answerable:\n{}",
        logged.dump(),
    );
}

#[tokio::test]
async fn a_blob_that_is_there_logs_nothing() {
    // The line above is only useful if it means something. A warning on every
    // read would be noise, and noise is how a real one goes unread.
    let blobs = BlobStore::in_memory(1024 * 1024);
    let hash = "3fc2a1d0b7e6459c8a2f1e0d9c8b7a6554433221100ffeeddccbbaa9988776655";
    blobs
        .put("tenant-x", hash, bytes::Bytes::from_static(b"hello"))
        .await
        .unwrap();

    let logged = Captured::default();
    let subscriber = tracing_subscriber::registry().with(logged.clone());
    let bytes = with_default(subscriber, || {
        futures::executor::block_on(blobs.get("tenant-x", hash))
    })
    .unwrap();

    assert_eq!(bytes.as_ref(), b"hello");
    assert!(
        !logged.contains("missing from the blob store"),
        "a normal read is silent:\n{}",
        logged.dump(),
    );
}
