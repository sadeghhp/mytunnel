//! Connection slab allocator
//!
//! Pre-allocated connection slots with O(1) allocation and deallocation.
//! Uses a bitset for tracking free slots.

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Handle to a slot in the slab
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlabHandle(usize);

impl SlabHandle {
    /// Get the raw index
    pub fn index(&self) -> usize {
        self.0
    }
}

/// A slab allocator for connection state
pub struct ConnectionSlab<T> {
    slots: Box<[Mutex<Option<T>>]>,
    /// Bitset tracking free slots (1 = free, 0 = occupied)
    /// Each u64 tracks 64 slots
    free_bitset: Box<[AtomicU64]>,
    /// Number of allocated slots
    allocated: AtomicUsize,
    /// Total capacity
    capacity: usize,
}

impl<T> ConnectionSlab<T> {
    /// Create a new slab with the given capacity
    pub fn new(capacity: usize) -> Self {
        // Round up to multiple of 64 for bitset
        let num_words = (capacity + 63) / 64;
        let actual_capacity = num_words * 64;

        // Initialize slots
        let slots: Vec<Mutex<Option<T>>> = (0..actual_capacity)
            .map(|_| Mutex::new(None))
            .collect();

        // Initialize bitset with all slots free (all 1s)
        let free_bitset: Vec<AtomicU64> = (0..num_words)
            .map(|i| {
                // Mark slots beyond capacity as occupied
                if i == num_words - 1 && capacity % 64 != 0 {
                    let valid_bits = capacity % 64;
                    AtomicU64::new((1u64 << valid_bits) - 1)
                } else if i * 64 < capacity {
                    AtomicU64::new(u64::MAX)
                } else {
                    AtomicU64::new(0)
                }
            })
            .collect();

        Self {
            slots: slots.into_boxed_slice(),
            free_bitset: free_bitset.into_boxed_slice(),
            allocated: AtomicUsize::new(0),
            capacity,
        }
    }

    /// Allocate a slot and insert value
    /// Returns None if slab is full
    pub fn insert(&self, value: T) -> Option<SlabHandle> {
        // Find a free slot using bitset
        for (word_idx, word) in self.free_bitset.iter().enumerate() {
            loop {
                let current = word.load(Ordering::Acquire);
                if current == 0 {
                    // No free slots in this word
                    break;
                }

                // Find first set bit (free slot)
                let bit_idx = current.trailing_zeros() as usize;
                let slot_idx = word_idx * 64 + bit_idx;

                if slot_idx >= self.capacity {
                    break;
                }

                // Try to claim this slot
                let mask = 1u64 << bit_idx;
                let new_value = current & !mask;

                if word
                    .compare_exchange(current, new_value, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    // Successfully claimed the slot
                    let mut slot = self.slots[slot_idx].lock();
                    *slot = Some(value);
                    self.allocated.fetch_add(1, Ordering::Relaxed);
                    return Some(SlabHandle(slot_idx));
                }
                // CAS failed, retry with updated value
            }
        }

        None // Slab is full
    }

    /// Remove and return value at handle
    pub fn remove(&self, handle: SlabHandle) -> Option<T> {
        let idx = handle.0;
        if idx >= self.capacity {
            return None;
        }

        let mut slot = self.slots[idx].lock();
        let value = slot.take()?;

        // Mark slot as free in bitset
        let word_idx = idx / 64;
        let bit_idx = idx % 64;
        let mask = 1u64 << bit_idx;
        self.free_bitset[word_idx].fetch_or(mask, Ordering::Release);
        self.allocated.fetch_sub(1, Ordering::Relaxed);

        Some(value)
    }

    /// Get reference to value at handle
    pub fn get(&self, handle: SlabHandle) -> Option<parking_lot::MappedMutexGuard<'_, T>> {
        let idx = handle.0;
        if idx >= self.capacity {
            return None;
        }

        let guard = self.slots[idx].lock();
        if guard.is_some() {
            Some(parking_lot::MutexGuard::map(guard, |opt| {
                opt.as_mut().unwrap()
            }))
        } else {
            None
        }
    }

    /// Get mutable reference to value at handle
    pub fn get_mut(&self, handle: SlabHandle) -> Option<parking_lot::MappedMutexGuard<'_, T>> {
        let idx = handle.0;
        if idx >= self.capacity {
            return None;
        }

        let guard = self.slots[idx].lock();
        if guard.is_some() {
            Some(parking_lot::MutexGuard::map(guard, |opt| {
                opt.as_mut().unwrap()
            }))
        } else {
            None
        }
    }

    /// Get current allocation count
    pub fn len(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    /// Check if slab is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get total capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Check if slab is full
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slab_insert_remove() {
        let slab: ConnectionSlab<u64> = ConnectionSlab::new(100);

        let h1 = slab.insert(42).unwrap();
        let h2 = slab.insert(100).unwrap();

        assert_eq!(slab.len(), 2);
        assert_eq!(*slab.get(h1).unwrap(), 42);
        assert_eq!(*slab.get(h2).unwrap(), 100);

        assert_eq!(slab.remove(h1), Some(42));
        assert_eq!(slab.len(), 1);
        assert!(slab.get(h1).is_none());
    }

    #[test]
    fn test_slab_reuse() {
        let slab: ConnectionSlab<u64> = ConnectionSlab::new(2);

        let h1 = slab.insert(1).unwrap();
        let _h2 = slab.insert(2).unwrap();
        assert!(slab.insert(3).is_none()); // Full

        slab.remove(h1);
        let h3 = slab.insert(3).unwrap();
        assert_eq!(h3.index(), h1.index()); // Reused slot
    }
}

