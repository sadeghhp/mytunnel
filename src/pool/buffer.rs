//! Fixed-size buffer pool
//!
//! Pre-allocated buffers with lock-free acquire/release for zero-allocation
//! data forwarding in the hot path.

use crossbeam::queue::ArrayQueue;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Buffer size tiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSize {
    /// 4KB - for small packets and headers
    Small = 4096,
    /// 16KB - for typical data transfers
    Medium = 16384,
    /// 64KB - for large transfers and batching
    Large = 65536,
}

impl BufferSize {
    pub fn as_usize(self) -> usize {
        self as usize
    }
}

/// A buffer from the pool
pub struct Buffer {
    data: Box<[u8]>,
    size: BufferSize,
    pool: Arc<BufferPoolInner>,
}

impl Buffer {
    /// Get the buffer's capacity
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    /// Get the buffer size tier
    pub fn size_tier(&self) -> BufferSize {
        self.size
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // Return buffer to pool
        let data = std::mem::replace(&mut self.data, Box::new([]));
        self.pool.return_buffer(data, self.size);
    }
}

/// Inner pool state (shared across clones)
struct BufferPoolInner {
    small_buffers: ArrayQueue<Box<[u8]>>,
    medium_buffers: ArrayQueue<Box<[u8]>>,
    large_buffers: ArrayQueue<Box<[u8]>>,
    
    // Metrics
    small_allocated: AtomicUsize,
    medium_allocated: AtomicUsize,
    large_allocated: AtomicUsize,
    small_in_use: AtomicUsize,
    medium_in_use: AtomicUsize,
    large_in_use: AtomicUsize,
}

impl BufferPoolInner {
    fn return_buffer(&self, data: Box<[u8]>, size: BufferSize) {
        match size {
            BufferSize::Small => {
                self.small_in_use.fetch_sub(1, Ordering::Relaxed);
                let _ = self.small_buffers.push(data);
            }
            BufferSize::Medium => {
                self.medium_in_use.fetch_sub(1, Ordering::Relaxed);
                let _ = self.medium_buffers.push(data);
            }
            BufferSize::Large => {
                self.large_in_use.fetch_sub(1, Ordering::Relaxed);
                let _ = self.large_buffers.push(data);
            }
        }
    }
}

/// Lock-free buffer pool with pre-allocated buffers
#[derive(Clone)]
pub struct BufferPool {
    inner: Arc<BufferPoolInner>,
}

impl BufferPool {
    /// Create a new buffer pool with pre-allocated buffers
    pub fn new(small_count: usize, medium_count: usize, large_count: usize) -> Self {
        let inner = BufferPoolInner {
            small_buffers: ArrayQueue::new(small_count),
            medium_buffers: ArrayQueue::new(medium_count),
            large_buffers: ArrayQueue::new(large_count),
            small_allocated: AtomicUsize::new(0),
            medium_allocated: AtomicUsize::new(0),
            large_allocated: AtomicUsize::new(0),
            small_in_use: AtomicUsize::new(0),
            medium_in_use: AtomicUsize::new(0),
            large_in_use: AtomicUsize::new(0),
        };

        // Pre-allocate buffers
        for _ in 0..small_count {
            let buf = vec![0u8; BufferSize::Small.as_usize()].into_boxed_slice();
            let _ = inner.small_buffers.push(buf);
            inner.small_allocated.fetch_add(1, Ordering::Relaxed);
        }

        for _ in 0..medium_count {
            let buf = vec![0u8; BufferSize::Medium.as_usize()].into_boxed_slice();
            let _ = inner.medium_buffers.push(buf);
            inner.medium_allocated.fetch_add(1, Ordering::Relaxed);
        }

        for _ in 0..large_count {
            let buf = vec![0u8; BufferSize::Large.as_usize()].into_boxed_slice();
            let _ = inner.large_buffers.push(buf);
            inner.large_allocated.fetch_add(1, Ordering::Relaxed);
        }

        Self {
            inner: Arc::new(inner),
        }
    }

    /// Acquire a buffer of the specified size
    /// Returns None if pool is exhausted (caller should retry or allocate)
    pub fn acquire(&self, size: BufferSize) -> Option<Buffer> {
        let (queue, in_use) = match size {
            BufferSize::Small => (&self.inner.small_buffers, &self.inner.small_in_use),
            BufferSize::Medium => (&self.inner.medium_buffers, &self.inner.medium_in_use),
            BufferSize::Large => (&self.inner.large_buffers, &self.inner.large_in_use),
        };

        queue.pop().map(|data| {
            in_use.fetch_add(1, Ordering::Relaxed);
            Buffer {
                data,
                size,
                pool: self.inner.clone(),
            }
        })
    }

    /// Acquire a buffer, allocating a new one if pool is exhausted
    pub fn acquire_or_alloc(&self, size: BufferSize) -> Buffer {
        self.acquire(size).unwrap_or_else(|| {
            // Pool exhausted, allocate new buffer (not ideal but prevents failure)
            let data = vec![0u8; size.as_usize()].into_boxed_slice();
            Buffer {
                data,
                size,
                pool: self.inner.clone(),
            }
        })
    }

    /// Get pool statistics
    pub fn stats(&self) -> BufferPoolStats {
        BufferPoolStats {
            small_allocated: self.inner.small_allocated.load(Ordering::Relaxed),
            small_in_use: self.inner.small_in_use.load(Ordering::Relaxed),
            medium_allocated: self.inner.medium_allocated.load(Ordering::Relaxed),
            medium_in_use: self.inner.medium_in_use.load(Ordering::Relaxed),
            large_allocated: self.inner.large_allocated.load(Ordering::Relaxed),
            large_in_use: self.inner.large_in_use.load(Ordering::Relaxed),
        }
    }
}

/// Buffer pool statistics
#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    pub small_allocated: usize,
    pub small_in_use: usize,
    pub medium_allocated: usize,
    pub medium_in_use: usize,
    pub large_allocated: usize,
    pub large_in_use: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_acquire_release() {
        let pool = BufferPool::new(10, 5, 2);
        
        // Acquire a buffer
        let buf = pool.acquire(BufferSize::Small).unwrap();
        assert_eq!(buf.capacity(), 4096);
        
        let stats = pool.stats();
        assert_eq!(stats.small_in_use, 1);
        
        // Drop returns to pool
        drop(buf);
        
        let stats = pool.stats();
        assert_eq!(stats.small_in_use, 0);
    }

    #[test]
    fn test_buffer_pool_exhaustion() {
        let pool = BufferPool::new(2, 1, 1);
        
        let _b1 = pool.acquire(BufferSize::Small).unwrap();
        let _b2 = pool.acquire(BufferSize::Small).unwrap();
        
        // Pool exhausted for small buffers
        assert!(pool.acquire(BufferSize::Small).is_none());
        
        // But acquire_or_alloc still works
        let _b3 = pool.acquire_or_alloc(BufferSize::Small);
    }
}

