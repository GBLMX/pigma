//! Taking a lock without letting a poisoned lock take the process down with it.
//!
//! `std`'s locks poison themselves when the thread that held them panics, and every call site
//! then has to answer the same question: what does that panic mean for the value underneath?
//! Nearly everywhere in this app the answer is "nothing" — the guarded value is a snapshot, a
//! cache, or a slot that another thread's panic says nothing about — so the right move is to
//! take the data and carry on. Turning one thread's failure into a process-wide one is strictly
//! worse: a poisoned IPC lock used to abort the whole app on the next `status` poll.
//!
//! Writing that decision once here is what keeps the `.unwrap_or_else(|e| e.into_inner())` from
//! drifting call site to call site, and is why the four IPC sites and the cache sites now read
//! the same. The exceptions keep their own handling: `playback::player` turns a poisoned audio
//! lock into an `io::Error` because its callers have that channel, and a lock whose contents are
//! only meaningful if no writer died mid-update would want the opposite of this.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Lock `mutex`, ignoring poisoning.
pub fn mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Take `lock` for reading, ignoring poisoning.
pub fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Take `lock` for writing, ignoring poisoning.
pub fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
