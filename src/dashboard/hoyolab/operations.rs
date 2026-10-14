//! One in-flight upstream operation per owner, never locked across retry delays.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use uuid::Uuid;

#[derive(Clone, Default)]
pub(in crate::dashboard) struct Operations {
    owners: Arc<Mutex<HashMap<Uuid, Weak<AsyncMutex<()>>>>>,
}
impl Operations {
    pub(in crate::dashboard) async fn acquire(&self, owner: Uuid) -> OwnedMutexGuard<()> {
        let lock = {
            let mut owners = self.owners.lock().unwrap_or_else(|p| p.into_inner());
            // ponytail: scan active owners; explicit cleanup if the private service grows.
            owners.retain(|_, value| value.strong_count() > 0);
            match owners.get(&owner).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(AsyncMutex::new(()));
                    owners.insert(owner, Arc::downgrade(&lock));
                    lock
                }
            }
        };
        lock.lock_owned().await
    }
}

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
