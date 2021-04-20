#![feature(ptr_internals)]
#![feature(allocator_api)]
#![feature(nonnull_slice_from_raw_parts)]

mod consts;
mod function;
mod structs;

use function::*;
use std::{
    alloc::{self, GlobalAlloc},
    cell::RefCell,
    mem,
    ptr::{self, null_mut, NonNull},
};
use structs::{AreaInfo, BlockHeader, FreeNode, TLSFChunk, TLSFRawHeader, TLSFRootChunk};

extern crate arrayvec;
use arrayvec::ArrayVec;

extern crate spin;
use spin::Mutex;

/// TLSF root pool.
struct RootPool {
    memory: TLSFRootChunk,
}

impl RootPool {
    /// Get tlsf header as mut from chunk memory buffer.
    fn tlsf_header(&self) -> &mut TLSFRawHeader {
        unsafe {
            (self.memory.ptr().as_ptr() as *mut TLSFRawHeader)
                .as_mut()
                .unwrap()
        }
    }

    /// Create TLSF memory pool with given requested size.
    ///
    /// Successfully returned value is pool instance and actually allocable memory size.
    ///
    /// # Arguments
    ///
    /// * 'requested_size' - Memory request size.
    pub fn from(requested_size: usize) -> Option<Self> {
        const MINIMUM_REQUIRED_SIZE: usize = TLSFRawHeader::get_aligned_size()
            + (BlockHeader::get_aligned_size() * 3)
            + AreaInfo::get_aligned_size();

        // If requested size is 0, do nothing or check miminum required size of memroy pool.
        if requested_size < MINIMUM_REQUIRED_SIZE {
            return None;
        }

        // Allocate memory (Should be 16 byte aligned.)
        // In windows, Default syst()em allocation calls HeapAlloc, not VirtualAlloc.
        // @todo We should allocate memory using VirtualAlloc if can.
        let new_chunk = TLSFRootChunk::new(requested_size)?;
        let first_block = unsafe {
            // Get start block header pointer and write area info.
            let offset = TLSFRawHeader::get_aligned_size() as isize;
            (new_chunk.ptr().as_ptr().offset(offset) as *mut BlockHeader)
                .as_mut()
                .unwrap()
                .next_block_as_mut()
        };

        // Set field and move memory to outside.
        let pool = Self { memory: new_chunk };

        let tlsf_header = pool.tlsf_header();
        tlsf_header.maximum_memory_size = first_block.buffer_size_with_header();
        tlsf_header.used_memory_size += tlsf_header.maximum_memory_size;

        // Make first block of the memory pool.
        // We have to free first_block_header's memory pool manually to fit memory usage and store item into array.
        unsafe {
            pool.dealloc(
                first_block.buffer_as_ptr().unwrap().as_ptr(),
                alloc::Layout::new::<u8>(),
            );
        }

        Some(pool)
    }
}

unsafe impl alloc::GlobalAlloc for RootPool {
    unsafe fn alloc(&self, layout: alloc::Layout) -> *mut u8 {
        // Find suitable block index.
        let aligned_size = calculate_allocation_searching_size(layout.size());

        let tlsf_header = self.tlsf_header();
        let mapping_indices = match tlsf_header.find_suitable_indices(aligned_size) {
            None => return null_mut(),
            Some(mapping_indices) => mapping_indices,
        };

        // Extract block from free-block map.
        let suitable_block = match tlsf_header.extract_root_block(mapping_indices) {
            None => return null_mut(),
            Some(mut suitable_block) => suitable_block.as_mut(),
        };
        assert!(
            suitable_block.buffer_size() >= aligned_size,
            "Buffer size of retrieved block must be larger or equal to aligned size."
        );

        // Check there is remained block which can be merged to next block or separated.
        // Check remained size can be independent another block.
        const BLOCK_SIZE: usize = BlockHeader::get_aligned_size() + mem::size_of::<FreeNode>();
        let remained_size = suitable_block.buffer_size() - aligned_size;
        if remained_size < BLOCK_SIZE {
            // If remained size can not be another block, just set flag to next block.
            suitable_block.next_block_as_mut().set_previous_freed(false);
        } else {
            // Find the pointer of new another block and write new information for block.
            let new_buffer_size = remained_size - BlockHeader::get_aligned_size();
            let new_block = {
                let new_block = suitable_block
                    .buffer_pointer_as::<u8>()
                    .offset(aligned_size as isize);
                ptr::write(
                    new_block as *mut _,
                    BlockHeader::new(new_buffer_size, true, false, None),
                );
                (new_block as *const BlockHeader).as_ref().unwrap()
            };

            // Get original next block and update information.
            let new_block_ptr = NonNull::new(new_block as *const _ as *mut _).unwrap();
            let orig_next_block = suitable_block.next_block_as_mut();
            orig_next_block.set_previous_header(new_block_ptr);
            suitable_block.set_buffer_size(aligned_size);

            let mapping_indices = calculate_mapping_indices(new_buffer_size);
            tlsf_header.insert_block(new_block_ptr, mapping_indices);
        }

        // Update allocated block's flag and header data.
        // Add memory usage by block size to be used and additional header size.
        suitable_block.set_freed(false);
        tlsf_header.used_memory_size += suitable_block.buffer_size_with_header();

        // Return buffer slice.
        suitable_block.buffer_pointer_as::<u8>() as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: alloc::Layout) {
        // Backward pointer to find 'BlockHeader'
        let block = {
            (ptr.offset(-(BlockHeader::get_aligned_size() as isize)) as *mut BlockHeader)
                .as_mut()
                .unwrap()
        };
        block.set_freed(true);

        // Update flag and reset buffer as freed_block next to the header.
        let tlsf_header = self.tlsf_header();
        tlsf_header.used_memory_size -= block.buffer_size_with_header();
        {
            let freed_block = block.buffer_pointer_as::<FreeNode>() as *mut FreeNode;
            ptr::write(freed_block, FreeNode::new());
        }

        // Get next block and merge it when next block is exist and freed.
        {
            let next_block = block.next_block_as_mut();
            if next_block.is_freed() {
                let additonal_block_size = next_block.buffer_size_with_header();
                tlsf_header
                    .extract_freed_block(NonNull::new(next_block as *mut BlockHeader).unwrap());

                // Combine available size.
                block.set_buffer_size(block.buffer_size() + additonal_block_size);
            }
        }

        // Get previous block and merge it when prev block is exist and freed.
        if block.is_prev_freed() {
            let mut prev_block_ptr = block.previous_block_ptr().unwrap();
            tlsf_header.extract_freed_block(prev_block_ptr);

            // Insert prev_block instead of block.
            let prev_block = prev_block_ptr.as_mut();
            prev_block.set_buffer_size(prev_block.buffer_size() + block.buffer_size_with_header());

            let mapping_indices = calculate_mapping_indices(prev_block.buffer_size());
            tlsf_header.insert_block(prev_block_ptr, mapping_indices);

            // Chain to prev-next block with previous block.
            let prev_next_block = prev_block.next_block_as_mut();
            prev_next_block.set_previous_freed(true);
            prev_next_block.set_previous_header(prev_block_ptr);
        } else {
            let block_ptr = NonNull::new(block as *mut BlockHeader).unwrap();
            tlsf_header.insert_block(block_ptr, calculate_mapping_indices(block.buffer_size()));

            // Chain to next block with block.
            let next_block = block.next_block_as_mut();
            next_block.set_previous_freed(true);
            next_block.set_previous_header(block_ptr);
        }
    }
}

///
///
///
struct DynamicPool {
    root_pool: RefCell<Option<RootPool>>,
    additional_chunks: RefCell<ArrayVec<Option<TLSFChunk>, 32usize>>,
}

impl DynamicPool {
    const fn new() -> Self {
        Self {
            root_pool: RefCell::new(None),
            additional_chunks: RefCell::new(ArrayVec::<_, 32>::new_const()),
        }
    }
}

unsafe impl alloc::GlobalAlloc for DynamicPool {
    unsafe fn alloc(&self, layout: alloc::Layout) -> *mut u8 {
        // If root pool is not exist, make new one.
        // This must be successful.
        if self.root_pool.borrow().is_none() {
            self.root_pool.replace(Some(
                RootPool::from(next_chunk_size(0, 0, layout.pad_to_align().size())).unwrap(),
            ));
        }

        // Try allocation.
        // `alloc` function force to use mutable varible using `RefCell`.
        let mut borrowed_root_pool = self.root_pool.borrow_mut();
        let root_pool = borrowed_root_pool.as_mut().unwrap();

        let mut new_pool_created = false;
        let buffer: Option<*mut u8> = loop {
            let buffer_ptr = root_pool.alloc(layout);
            if buffer_ptr.is_null() == false {
                break Some(buffer_ptr);
            }

            // vvv Failure code. If new pool is created but failed to allocate again, just do nothing.
            if new_pool_created {
                break None;
            }

            // If allocation is failed, try make new chunk.
            // Check the cursor is about to be ouf of range. If true, we can not allocate anymore.
            let (len, capacity) = {
                let borrow = self.additional_chunks.borrow();
                (borrow.len(), borrow.capacity())
            };
            if len >= capacity {
                break None;
            }

            // Get last chunk size for calculate new chunk size.
            let tlsf_header = root_pool.tlsf_header();
            let last_chunk_size = {
                if len == 0 {
                    tlsf_header.maximum_memory_size
                } else {
                    let ref_chunks = self.additional_chunks.borrow();
                    ref_chunks[len - 1].as_ref().unwrap().layout.size()
                }
            };

            // Create next chunk.
            let new_chunk_size = next_chunk_size(
                tlsf_header.maximum_memory_size,
                last_chunk_size,
                calculate_allocation_size(layout.size()),
            );
            let new_chunk = match TLSFChunk::new(new_chunk_size) {
                None => break None, // Creation of new TLSFChunk may be failed by allocation.
                Some(new_chunk) => new_chunk,
            };

            // Register new chunk at first.
            let mut chunk_list = self.additional_chunks.borrow_mut();
            chunk_list.push(Some(new_chunk));
            let back_index = chunk_list.len() - 1;

            // Add new chunk's biggest buffer into the map.
            let used_chunk = tlsf_header.add_new_chunk(chunk_list[back_index].as_mut().unwrap());
            root_pool.dealloc(used_chunk.unwrap().as_ptr(), layout);
            new_pool_created = true;
        };

        // Return.
        match buffer {
            None => null_mut(),
            Some(ptr) => ptr,
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: alloc::Layout) {
        assert!(ptr.is_null() == false, "");
        assert!(self.root_pool.borrow().is_some(), "");

        self.root_pool
            .borrow()
            .as_ref()
            .unwrap()
            .dealloc(ptr, layout);
    }
}

/// Dynamic expandable TLSF memory allocator.
///
/// Can be used by specifying it as `#[global_allocator]`.
pub struct TLSFAllocator {
    pool: Mutex<DynamicPool>,
}

impl TLSFAllocator {
    pub const fn new() -> Self {
        Self {
            pool: Mutex::new(DynamicPool::new()),
        }
    }
}

unsafe impl alloc::GlobalAlloc for TLSFAllocator {
    unsafe fn alloc(&self, layout: alloc::Layout) -> *mut u8 {
        // Request allocation.
        self.pool.lock().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: alloc::Layout) {
        self.pool.lock().dealloc(ptr, layout);
    }
}

impl Drop for TLSFAllocator {
    fn drop(&mut self) {}
}
