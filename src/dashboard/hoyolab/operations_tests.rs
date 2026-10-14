use super::*;
use std::{
    future::{Future, poll_fn},
    task::Poll,
};
#[tokio::test]
async fn only_the_same_owner_waits_and_unused_locks_are_reclaimed() {
    let operations = Operations::default();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let first = operations.acquire(a).await;
    let mut waiting = Box::pin(operations.acquire(a));
    poll_fn(|cx| {
        assert!(waiting.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let other = operations.acquire(b).await;
    drop(other);
    drop(first);
    let second = waiting.await;
    drop(second);
    let final_guard = operations.acquire(b).await;
    assert_eq!(operations.owners.lock().unwrap().len(), 1);
    drop(final_guard);
}
