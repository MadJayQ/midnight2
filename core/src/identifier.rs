//! Defines types and APIs for creating globally unique identifiers in the engine.

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

#[derive(Copy, Clone, PartialEq, Debug, Hash)]
// Global IDs are atomically guaranteed to be unique across all threads in an application,
// Their primary use case is to allocate global IDs where uniqueness is required across multiple threads
// ECS World is a primary example. Once an ID has been handed out, it will NEVER be allocated to another caller
// Even if the original owner of that ID is gone.
pub struct GlobalId(usize);

static ALLOCATED_GLOBAL_ID: AtomicUsize = AtomicUsize::new(0);

impl GlobalId {
    pub fn allocate() -> Option<Self> {
        ALLOCATED_GLOBAL_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |val| {
                val.checked_add(1)
            })
            .map(GlobalId)
            .ok()
    }
}

// ThreadLocal Ids are atomically guaranteed to be unique within a given thread, they should NEVER be used
// On another thread for obvious reasons, maybe this construct is actually a bad idea because of that?
#[derive(Copy, Clone, PartialEq, Debug, Hash)]
pub struct ThreadLocalId(usize);

thread_local! {
    static ALLOCATED_THREAD_ID: AtomicUsize = AtomicUsize::new(0);  
}

impl ThreadLocalId {
    pub fn allocate() -> Option<Self> {
        ALLOCATED_THREAD_ID.with(|thread_id|
            thread_id.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |val| {
                val.checked_add(1)
            })
            .map(ThreadLocalId)
            .ok())
    }
} 

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_ids_unique() {
        let ids = std::iter::repeat_with(GlobalId::allocate)
            .take(50)
            .map(Option::unwrap)
            .collect::<Vec<_>>();
        for (i, &id1) in ids.iter().enumerate() {
            // For the first element, i is 0 - so skip 1
            for &id2 in ids.iter().skip(i + 1) {
                assert_ne!(id1, id2, "WorldIds should not repeat");
            }
        }
    }
}
