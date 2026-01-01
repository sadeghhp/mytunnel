//! Memory pool management
//!
//! Pre-allocated memory pools for zero-allocation hot paths.

mod buffer;
mod slab;

pub use buffer::{Buffer, BufferPool, BufferSize};
pub use slab::{ConnectionSlab, SlabHandle};

