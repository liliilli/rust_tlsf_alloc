#![allow(dead_code)]
use alloc::Allocator;

use super::{consts::*, function::*};
use std::{
    alloc, mem,
    ptr::{self, NonNull},
};

/// Indicates previous or next block pointer.
///
/// This type must be used only when owned block is freed.
pub struct FreeNode {
    pub prev: Option<NonNull<BlockHeader>>,
    pub next: Option<NonNull<BlockHeader>>,
}

impl FreeNode {
    pub fn new() -> Self {
        Self {
            prev: None,
            next: None,
        }
    }
}

/// Header that precedes to actual buffer memory in TLSF chunk.
pub struct BlockHeader {
    /// Previous header pointer.
    previous_header: Option<NonNull<BlockHeader>>,
    /// Stored size and other flag in 0b111 bits section.
    stored_size: usize,
}

impl BlockHeader {
    /// Mask for representing which block is free.
    const FREED_MASK: usize = 0x01;
    /// Make for representing previous block of arbitrary block is free.
    const PREV_FREED_MASK: usize = 0x02;

    /// Calculate the value which combined buffer memory size with bit-flags.
    ///
    /// # Arguments
    ///
    /// * 'buffer_size' - The size of memory buffer. Given value will be aligned up to
    /// 'BLOCK_SIZE'.
    /// * 'is_freed' - The flag which indicates given block is freed or not.
    /// * 'is_prev_freed' - The flag which indicates previous block is freed or not.
    fn calculate_stored_size(buffer_size: usize, is_freed: bool, is_prev_freed: bool) -> usize {
        // Round up size and leave it as 0xXXXX0000
        // Least 4 bits are emptied and reused as updating flags.
        let size = round_up_block(buffer_size);
        let flags = {
            let freed_mask = if is_freed { Self::FREED_MASK } else { 0x00 };
            let prev_freed_mask = if is_prev_freed {
                Self::PREV_FREED_MASK
            } else {
                0x00
            };
            freed_mask | prev_freed_mask
        };
        // Combine aligned size and flags.
        size | flags
    }

    /// Get aligned memory size of `BlockHeader`.
    pub const fn get_aligned_size() -> usize {
        round_up_block(mem::size_of::<BlockHeader>())
    }

    /// Create new block header.
    ///
    /// # Arguments
    ///
    /// * 'buffer_size' - The size of memory buffer. Given size will be aligned up to 'BLOCK_SIZE'.
    /// * 'is_freed' - The flag which indicates given block is freed or not.
    /// * 'is_prev_freed' - The flag which indicates previous block is freed or not.
    /// * 'previous_header' - Optional previous block header pointer.
    pub fn new(
        buffer_size: usize,
        is_freed: bool,
        is_prev_freed: bool,
        previous_header: Option<NonNull<BlockHeader>>,
    ) -> Self {
        Self {
            previous_header,
            stored_size: Self::calculate_stored_size(buffer_size, is_freed, is_prev_freed),
        }
    }

    /// Create new block header but for start block of TLSF chunk.
    pub fn new_start_block() -> Self {
        // Setup new information.
        // Have to set block header as chunk which has start area information buffer.
        let areainfo_buffer_size = calculate_allocation_size(mem::size_of::<AreaInfo>());
        Self::new(areainfo_buffer_size, false, false, None)
    }

    /// Get just block size without any flags.
    pub fn buffer_size(&self) -> usize {
        // Just reuse round down function to remove flags in 0b111 section.
        // member stored_size always be aligned since has been stored in item.
        round_down_block(self.stored_size)
    }

    /// Get block header and buffer combined byte size.
    pub fn buffer_size_with_header(&self) -> usize {
        Self::get_aligned_size() + self.buffer_size()
    }

    /// Get the next block header as a reference.
    /// Next block must be initialized. Otherwise, UB may be occurred.
    pub fn next_block_as_ref(&self) -> &Self {
        unsafe { self.next_block_ptr().as_ref() }
    }

    /// Get the next block header as a reference.
    /// Next block must be initialized. Otherwise, UB may be occurred.
    pub fn next_block_as_mut(&mut self) -> &mut Self {
        unsafe { self.next_block_ptr().as_mut() }
    }

    /// Get the pointer of next block header from this.
    /// Returned pointer may be initialized or not.
    pub unsafe fn next_block_ptr(&self) -> NonNull<BlockHeader> {
        let ptr: *const u8 = (self as *const Self) as *const u8;
        let offset = (Self::get_aligned_size() + self.buffer_size()) as isize;
        NonNull::new(ptr.offset(offset) as *mut Self).unwrap()
    }

    /// Get buffer pointer but cast a pointer of another type.
    pub unsafe fn buffer_pointer_as<U>(&self) -> *const U {
        let ptr: *const u8 = (self as *const BlockHeader) as *const u8;
        let offset = (Self::get_aligned_size()) as isize;
        ptr.offset(offset) as *const U
    }

    /// Get buffer as 'FreeNode'.
    ///
    /// Buffer must be initialized with valid 'FreeNode' item. Otherwise, UB may be occurred.
    pub fn buffer_as_freenode_as_mut(&mut self) -> Option<&mut FreeNode> {
        if !self.is_freed() {
            None
        } else {
            unsafe { (self.buffer_pointer_as::<FreeNode>() as *mut FreeNode).as_mut() }
        }
    }

    /// Get buffer as 'AreaInfo'.
    ///
    /// Buffer must be initialized with valid 'AreaInfo' item. Otherwise, UB may be occurred.
    pub fn buffer_as_areainfo_as_mut(&mut self) -> Option<&mut AreaInfo> {
        if self.is_freed() {
            None
        } else {
            unsafe { (self.buffer_pointer_as::<AreaInfo>() as *mut AreaInfo).as_mut() }
        }
    }

    /// Get buffer pointer if this block is not freed yet.
    pub unsafe fn buffer_as_ptr(&self) -> Option<NonNull<u8>> {
        if self.is_freed() {
            None
        } else {
            NonNull::new(self.buffer_pointer_as::<u8>() as *mut u8)
        }
    }

    /// Check whether this block is freed or not.
    pub fn is_freed(&self) -> bool {
        self.stored_size & Self::FREED_MASK != 0
    }

    /// Check whether previous block is freed or not.
    pub fn is_prev_freed(&self) -> bool {
        self.stored_size & Self::PREV_FREED_MASK != 0
    }

    /// Get previous block pointer.
    /// Returned value may not have value.
    pub fn previous_block_ptr(&mut self) -> Option<NonNull<BlockHeader>> {
        self.previous_header
    }

    /// Set previous block.
    pub fn set_previous_header(&mut self, prev_block: NonNull<Self>) {
        self.previous_header = Some(prev_block);
    }

    /// Reset previous block.
    pub fn reset_previous_block(&mut self) {
        self.previous_header = None;
    }

    /// Set flag for whether this block is freed or not.
    ///
    /// # Arguments
    ///
    /// * 'is_free' - Flag which specifies that this block is freed or not.
    pub fn set_freed(&mut self, is_free: bool) {
        self.stored_size =
            Self::calculate_stored_size(self.buffer_size(), is_free, self.is_prev_freed());
    }

    /// Set flag for whether previous block is freed or not.
    ///
    /// # Arguments
    ///
    /// * 'is_prev_free' - Flag which specifies that previous block is freed.
    pub fn set_previous_freed(&mut self, is_prev_free: bool) {
        self.stored_size =
            Self::calculate_stored_size(self.buffer_size(), self.is_freed(), is_prev_free);
    }

    /// Set new buffer size.
    ///
    /// # Arguments
    ///
    /// * 'size' - 'BLOCK_SIZE' aligned new buffer size.
    pub fn set_buffer_size(&mut self, size: usize) {
        assert!(is_aligned(size));
        self.stored_size = Self::calculate_stored_size(size, self.is_freed(), self.is_prev_freed());
    }
}

///
pub struct AreaInfo {
    pub end_block_header: Option<NonNull<BlockHeader>>,
    pub next_area_header: Option<NonNull<AreaInfo>>,
}

impl AreaInfo {
    /// Get aligned memory size of `TSLFRawHeader`.
    pub const fn get_aligned_size() -> usize {
        round_up_block(mem::size_of::<AreaInfo>())
    }

    /// Create empty state item.
    pub fn new() -> Self {
        Self {
            end_block_header: None,
            next_area_header: None,
        }
    }
}

/// Manages freed block pointer into internal map.
///
/// This item does not own any of freed block item, just keeping pointer into container.
/// All functions should not create or share any ownershiped blocks.
#[derive(Debug, PartialEq)]
pub struct FreeNodeHeaderMap {
    map: [Option<NonNull<BlockHeader>>; TOTAL_COUNT],
}

impl FreeNodeHeaderMap {
    pub fn new() -> Self {
        Self {
            map: [None; TOTAL_COUNT],
        }
    }

    ///
    pub fn item_as_mut(&mut self, mapping_indices: (usize, usize)) -> Option<&mut BlockHeader> {
        let index = calculate_index(mapping_indices);
        if index >= TOTAL_COUNT {
            None
        } else {
            let item = &self.map[index];
            if item.is_none() {
                None
            } else {
                Some(unsafe { self.map[index].unwrap().as_mut() })
            }
        }
    }

    ///
    pub fn item_as_ref(&self, mapping_indices: (usize, usize)) -> Option<&BlockHeader> {
        let index = calculate_index(mapping_indices);
        if index >= TOTAL_COUNT {
            None
        } else {
            let item = &self.map[index];
            if item.is_none() {
                None
            } else {
                Some(unsafe { item.unwrap().as_ref() })
            }
        }
    }

    ///
    pub fn get_item(
        &self,
        mapping_indices: (usize, usize),
    ) -> Option<Option<NonNull<BlockHeader>>> {
        let index = calculate_index(mapping_indices);
        if index >= TOTAL_COUNT {
            None
        } else {
            Some(self.map[index])
        }
    }

    ///
    pub fn set_item(&mut self, mapping_indices: (usize, usize), block: NonNull<BlockHeader>) {
        let index = calculate_index(mapping_indices);
        assert!(index < TOTAL_COUNT);
        self.map[index] = Some(block);
    }

    ///
    pub fn reset_item(&mut self, mapping_indices: (usize, usize)) {
        let index = calculate_index(mapping_indices);
        assert!(index < TOTAL_COUNT);
        self.map[index] = None;
    }
}

#[derive(Debug, PartialEq)]
pub struct TLSFRawHeader {
    pub fl_bitmap: u32,
    pub sl_bitmap: [u32; FIRST_INDEX_REAL],
    pub areainfo_ptr: Option<NonNull<AreaInfo>>,
    pub freed_block_map: FreeNodeHeaderMap,
    pub maximum_memory_size: usize,
    pub used_memory_size: usize,
}

impl TLSFRawHeader {
    /// Get aligned memory size of `TSLFRawHeader`.
    pub const fn get_aligned_size() -> usize {
        round_up_block(mem::size_of::<TLSFRawHeader>())
    }

    ///
    pub fn new() -> Self {
        Self {
            fl_bitmap: 0,
            sl_bitmap: [0u32; FIRST_INDEX_REAL],
            areainfo_ptr: None,
            freed_block_map: FreeNodeHeaderMap::new(),
            maximum_memory_size: 0,
            used_memory_size: 0,
        }
    }

    ///
    pub fn insert_block(
        &mut self,
        mut block_ptr: NonNull<BlockHeader>,
        mapping_indices: (usize, usize),
    ) {
        let block = unsafe { block_ptr.as_mut() };
        assert!(block.is_freed(), "Block must be signed as freed.");

        // Make doubled linked list to original stored block and new block to be inserted in root.
        {
            // Connect original freed block pointer into new item.
            // next will be valid pointer or None.
            let freed_block: &mut FreeNode = block.buffer_as_freenode_as_mut().unwrap();
            freed_block.prev = None;
            freed_block.next = self.freed_block_map.get_item(mapping_indices).unwrap();

            // If indexing item of map has pointer,
            // Connect new item to original pointer.
            match self.freed_block_map.item_as_mut(mapping_indices) {
                None => (),
                Some(target_block) => {
                    let target_block =
                        unsafe { (target_block as *mut BlockHeader).as_mut() }.unwrap();
                    let freed_block = target_block.buffer_as_freenode_as_mut().unwrap();
                    freed_block.prev = Some(block_ptr);
                }
            }

            // Update value newly.
            self.freed_block_map.set_item(mapping_indices, block_ptr);
        }

        // Update flag.
        let (first, second) = mapping_indices;
        self.fl_bitmap |= 0x01 << (first & 0x1F);
        self.sl_bitmap[first] |= 0x01 << (second & 0x1F);
    }

    /// Find suitable indices (first, second) from given size.
    ///
    /// If size is too big to insert any freed item block,
    /// or there is no freed item in the container, just return 'None'.
    ///
    /// If found, return suitable mapping index (first, second).
    ///
    /// # Arguments
    ///
    /// * 'size' - Requested size to allocate.
    pub fn find_suitable_indices(&self, size: usize) -> Option<(usize, usize)> {
        // Align request size. Size will be aligned to 16 Bytes.
        let (first, second) = calculate_mapping_indices(calculate_allocation_size(size));

        let second_bitmask = (!0x0u32).overflowing_shl(second as u32).0;
        let second_masked_bits: u32 = self.sl_bitmap[first] & second_bitmask;
        if second_masked_bits > 0 {
            Some((first, calculate_lsb(second_masked_bits as usize).unwrap()))
        } else {
            let first_bitmask = (!0x0u32).overflowing_shl(first as u32 + 1).0;
            let first_masked_bits: u32 = self.fl_bitmap & first_bitmask;
            // If not found, just return function itself.
            if first_masked_bits <= 0 {
                None
            } else {
                let first = calculate_lsb(first_masked_bits as usize).unwrap();
                let second_masked_bits = self.sl_bitmap[first];
                Some((first, calculate_lsb(second_masked_bits as usize).unwrap()))
            }
        }
    }

    /// Extract block of matched indices (first, second).
    /// If not found, just return 'None'.
    ///
    /// If another block that chained to extract block is exist,
    /// the item will be overwritten to the socket where extracted block was in.
    ///
    /// # Arguments
    ///
    /// * 'mapping_indices' - Valid mapping indices to extract from.
    pub fn extract_root_block(
        &mut self,
        mapping_indices: (usize, usize),
    ) -> Option<NonNull<BlockHeader>> {
        let block_ptr = self.freed_block_map.get_item(mapping_indices).unwrap();
        if block_ptr.is_none() {
            return None;
        }

        let block = unsafe { block_ptr.unwrap().as_mut() };
        assert!(block.is_freed(), "Block must be signed as freed.");

        // Get next pointer (maybe) and clear block-freed.
        let block_freed = block.buffer_as_freenode_as_mut().unwrap();
        let next_block = block_freed.next;
        block_freed.next = None;
        block_freed.prev = None;

        // Match next_block (maybe).
        match next_block {
            None => {
                self.freed_block_map.reset_item(mapping_indices);

                // Clear bitflags.
                let (first, second) = mapping_indices;
                self.sl_bitmap[first] ^= 0x01 << (second & 0x1F);
                if self.sl_bitmap[first] == 0 {
                    self.fl_bitmap ^= 0x01 << (first & 0x1F);
                }
            }
            Some(next_block) => {
                let map = &mut self.freed_block_map;

                map.set_item(mapping_indices, next_block);
                map.item_as_mut(mapping_indices)
                    .unwrap()
                    .buffer_as_freenode_as_mut()
                    .unwrap()
                    .prev = None;
            }
        }

        block_ptr
    }

    ///
    ///
    /// # Arguments
    ///
    /// * 'block_ptr' -
    pub fn extract_freed_block(&mut self, mut block_ptr: NonNull<BlockHeader>) {
        // Check whether block is actually freed now.
        let block = unsafe { block_ptr.as_mut() };
        assert_eq!(block.is_freed(), true);

        // Discard chain between a neighborhoods.
        let next_block = {
            let freed_list = block.buffer_as_freenode_as_mut().unwrap();
            match freed_list.next {
                Some(mut next_block) => {
                    let next_block = unsafe { next_block.as_mut() };
                    next_block.buffer_as_freenode_as_mut().unwrap().prev = freed_list.prev;
                }
                _ => (),
            }
            match freed_list.prev {
                Some(mut prev_block) => {
                    let prev_block = unsafe { prev_block.as_mut() };
                    prev_block.buffer_as_freenode_as_mut().unwrap().next = freed_list.next;
                }
                _ => (),
            }
            freed_list.next
        };

        // Extract block if root item is same, and update bit-flags.
        let mapping_indices = calculate_mapping_indices(block.buffer_size());
        let block_in_map = self.freed_block_map.get_item(mapping_indices).unwrap();
        if block_in_map.is_some() {
            // If root item in free list is same to given block,
            // Update it to next block.
            let block_in_map = block_in_map.unwrap();
            if block_in_map == block_ptr {
                match next_block {
                    Some(next_block) => self.freed_block_map.set_item(mapping_indices, next_block),
                    None => {
                        self.freed_block_map.reset_item(mapping_indices);

                        // Clear bitflags.
                        let (first, second) = mapping_indices;
                        self.sl_bitmap[first] ^= 0x01 << (second & 0x1F);
                        if self.sl_bitmap[first] == 0 {
                            self.fl_bitmap ^= 0x01 << (first & 0x1F);
                        }
                    }
                }
            }
        }

        // Reset freed-list.
        let freed_list = block.buffer_as_freenode_as_mut().unwrap();
        freed_list.prev = None;
        freed_list.next = None;
    }

    ///
    /// ## Arguments
    ///
    /// * `new_chunk` - New memory chunk to append into TLSF pool.
    pub unsafe fn add_new_chunk<'a>(
        &'a mut self,
        new_chunk: &'a mut TLSFChunk,
    ) -> Option<NonNull<u8>> {
        let mut areainfo_cursor = self.areainfo_ptr;
        let mut previous_areainfo: Option<&mut AreaInfo> = None;

        let mut new_infoblock_ptr = new_chunk.ptr.as_ptr() as *mut BlockHeader;
        let mut new_firstblock_ptr = new_infoblock_ptr.as_mut()?.next_block_ptr().as_ptr();
        let mut new_endblock_ptr = new_firstblock_ptr.as_mut()?.next_block_ptr().as_ptr();

        while areainfo_cursor.is_some() {
            let old_infoblock = {
                let ptr = areainfo_cursor?
                    .cast::<u8>()
                    .as_ptr()
                    .offset(-(BlockHeader::get_aligned_size() as isize))
                    as *mut BlockHeader;
                ptr.as_mut().unwrap()
            };
            // If the address of buffer end of old buffer is same to new buffer's start, merge
            // together.
            let old_endblock = areainfo_cursor?.as_ref().end_block_header?.as_ref();
            let old_bufferend_addr = old_endblock.buffer_as_ptr()?.as_ptr() as usize;
            let old_bufferstt_addr = old_infoblock as *mut _ as usize;
            let new_bufferstt_addr = new_infoblock_ptr as usize;
            let new_bufferend_addr = new_endblock_ptr as usize;

            let old_firstblock = old_infoblock.next_block_as_mut();
            let old_endblock = areainfo_cursor?.as_mut().end_block_header?.as_mut();

            // Check and realign AreaInfo.
            // AreaInfo must be realigned.
            let is_blocks_neighbor = old_bufferend_addr == new_bufferstt_addr;
            let is_blocks_neighbor_reverse = old_bufferstt_addr == new_bufferend_addr;
            if is_blocks_neighbor || is_blocks_neighbor_reverse {
                if self.areainfo_ptr == areainfo_cursor {
                    let next_areainfo_ptr = areainfo_cursor?.as_ref().next_area_header;
                    self.areainfo_ptr = next_areainfo_ptr;
                    areainfo_cursor = next_areainfo_ptr;
                } else {
                    assert!(previous_areainfo.is_some(), "");
                    let next_areainfo_ptr = areainfo_cursor?.as_ref().next_area_header;
                    previous_areainfo.as_mut()?.next_area_header = next_areainfo_ptr;
                    areainfo_cursor = next_areainfo_ptr;
                }
            } else {
                previous_areainfo = Some(areainfo_cursor?.as_mut());
                areainfo_cursor = areainfo_cursor?.as_mut().next_area_header;
                continue;
            }

            if is_blocks_neighbor {
                // Merge
                let new_firstblock_size = new_firstblock_ptr.as_ref()?.buffer_size_with_header();
                let new_areainfo_size = new_infoblock_ptr.as_ref()?.buffer_size_with_header();
                old_endblock.set_buffer_size(new_firstblock_size + new_areainfo_size);

                // Set
                let old_endblock_ptr = NonNull::new(old_endblock as *mut _)?;
                let new_endblock = old_endblock.next_block_as_mut();
                new_endblock.set_previous_header(old_endblock_ptr);

                // Update
                new_firstblock_ptr = old_endblock as *mut _;
                new_infoblock_ptr = old_infoblock as *mut _;
            } else {
                // is_blocks_neighbor_reverse
                // Merge & Set
                let new_firstblock = new_firstblock_ptr.as_mut()?;
                old_firstblock.set_previous_header(NonNull::new(new_firstblock as *mut _)?);
                new_firstblock.set_buffer_size(
                    new_firstblock.buffer_size_with_header()
                        + old_infoblock.buffer_size_with_header(),
                );

                // Update
                new_endblock_ptr = old_endblock as *mut BlockHeader;
            }
        }

        // Insert the area in the list of linked areas.
        let final_areainfo = new_infoblock_ptr.as_mut()?.buffer_as_areainfo_as_mut()?;
        final_areainfo.next_area_header = self.areainfo_ptr;
        final_areainfo.end_block_header = NonNull::new(new_endblock_ptr);
        self.areainfo_ptr = NonNull::new(final_areainfo as *mut _);

        //
        let new_buffer_size = new_firstblock_ptr.as_mut()?.buffer_size_with_header();
        self.used_memory_size += new_buffer_size;
        self.maximum_memory_size += new_buffer_size;

        new_firstblock_ptr.as_mut()?.buffer_as_ptr()
    }
}

///
///
///
pub struct TLSFChunk {
    pub ptr: NonNull<u8>,
    pub layout: alloc::Layout,
}

unsafe impl Sync for TLSFChunk {}
unsafe impl Send for TLSFChunk {}

impl TLSFChunk {
    ///
    ///
    ///
    pub fn new_as_uninit(requested_size: usize) -> Option<Self> {
        // Allocate memory (Should be 16 byte aligned.)
        // In windows, Default syst()em allocation calls HeapAlloc, not VirtualAlloc.
        // @todo We should allocate memory using VirtualAlloc if can.
        use std::alloc::{Layout, System};
        let layout = Layout::array::<u8>(requested_size)
            .unwrap()
            .align_to(MINIMUM_BLOCK_SIZE)
            .unwrap();

        // Must be zeroed-allocated.
        // To allocate memory without using rust's allocation (to avoid recursive call),
        // we have to use libc's malloc.
        let ptr = match System.allocate_zeroed(layout) {
            Err(_) => return None,
            Ok(ptr) => unsafe { ptr.as_ref() }.as_ptr(),
        };
        assert!(
            is_aligned(ptr as usize) == true,
            "Must be aligned to BLOCK_SIZE."
        );
        Some(Self {
            ptr: NonNull::new(ptr as *mut _)?,
            layout,
        })
    }

    ///
    ///
    ///
    pub fn new(requested_size: usize) -> Option<Self> {
        let uninit_chunk = Self::new_as_uninit(requested_size)?;

        // Process area. (initialize_pool)
        let total_area_size = round_down_block(requested_size);
        assert!(
            is_aligned(total_area_size),
            "Total area size is not aligned properly."
        );

        // Get start block header pointer and write area info.
        initialize_pool(uninit_chunk.ptr.cast::<BlockHeader>(), total_area_size);
        Some(uninit_chunk)
    }
}

impl Drop for TLSFChunk {
    fn drop(&mut self) {
        use std::alloc::System;
        unsafe {
            System.deallocate(self.ptr, self.layout);
        }
    }
}

/// Initialize pool and construct basic blocks with headers.
///
/// # Arguments
///
/// * 'total_size' - Total buffer size which can be allocated without aligned TLSF header space.
///     input `total_size` must be aligned to BLOCK_ALIGNOF.
pub fn initialize_pool(mut start_block_ptr: NonNull<BlockHeader>, total_size: usize) {
    assert!(
        is_aligned(total_size),
        "Total area size is not aligned properly."
    );

    // Setup first block header.
    // Memory map will be like this,
    // [TLSFHeader...|BlockHeader...:AreaInfo...|NextBlockHeader...:Buffer...]
    let start_block: &mut BlockHeader = unsafe {
        // Get block header pointer and write area info.
        ptr::write(start_block_ptr.as_ptr(), BlockHeader::new_start_block());

        // Set AreaInfo in following buffer.
        let areainfo_ptr =
            start_block_ptr.as_ref().buffer_pointer_as::<AreaInfo>() as *mut AreaInfo;
        ptr::write(areainfo_ptr, AreaInfo::new());

        start_block_ptr.as_mut()
    };

    // Setup second block header.
    // Second block will be actual memory buffer which can be allocated to
    // any other instance which to be created.
    let buffer_size =
        total_size - start_block.buffer_size() - (3 * BlockHeader::get_aligned_size());
    let next_block: &BlockHeader = unsafe {
        // next_block should be check as false in initialization.
        // next_block will be freed manually, so registered into TSLF freed-item map.
        let next_block = start_block.next_block_ptr();
        ptr::write(
            next_block.as_ptr(),
            BlockHeader::new(buffer_size, false, false, None),
        );
        let next_block_buffer = next_block.as_ref().buffer_pointer_as::<FreeNode>();
        ptr::write(next_block_buffer as *mut _, FreeNode::new());

        next_block.as_ref()
    };

    // Setup end block header.
    // There is no extra buffer space following to end block header.
    // End block header must be not-freed state because when free other memory blocks
    // If following block was already freed, allocate would merge together.
    // End block header must not be merged.
    let end_block_ptr = unsafe {
        let end_block_ptr = next_block.next_block_ptr();
        ptr::write(
            end_block_ptr.as_ptr(),
            BlockHeader::new(
                0,
                false,
                true,
                NonNull::new(next_block as *const _ as *mut BlockHeader),
            ),
        );
        end_block_ptr
    };

    // Update area info header information having forwarded to end block.
    start_block
        .buffer_as_areainfo_as_mut()
        .unwrap()
        .end_block_header = Some(end_block_ptr);
}

pub struct TLSFRootChunk {
    chunk: TLSFChunk,
}

impl TLSFRootChunk {
    /// Create initialized root chunk of TLSF memory pool.
    pub fn new(requested_size: usize) -> Option<Self> {
        let chunk = TLSFChunk::new_as_uninit(requested_size)?;

        // Reset area information.
        // Write [0, size_of::<TlsfRaw>()) as TlsfRaw structure.
        // Don't care about internal TlsfRaw, will be discarded safely.
        let tlsf_header = unsafe {
            ptr::write(
                chunk.ptr.as_ptr() as *mut TLSFRawHeader,
                TLSFRawHeader::new(),
            );
            (chunk.ptr.as_ptr() as *mut TLSFRawHeader).as_mut()?
        };

        // Process area. (initialize_pool)
        let total_area_size = round_down_block(requested_size) - TLSFRawHeader::get_aligned_size();
        assert!(
            is_aligned(total_area_size),
            "Total area size is not aligned properly."
        );

        // Get start block header pointer and write area info.
        let mut start_block_ptr = unsafe {
            let offset = TLSFRawHeader::get_aligned_size() as isize;
            NonNull::new(chunk.ptr.as_ptr().offset(offset) as *mut BlockHeader)
        }
        .unwrap();
        initialize_pool(start_block_ptr, total_area_size);

        // Set areainfo pointer into header.
        tlsf_header.areainfo_ptr = unsafe {
            let ptr = start_block_ptr.as_mut().buffer_as_areainfo_as_mut()? as *mut AreaInfo;
            Some(NonNull::new(ptr)?)
        };

        Some(Self { chunk })
    }

    pub fn ptr(&self) -> NonNull<u8> {
        NonNull::new(self.chunk.ptr.as_ptr()).unwrap()
    }
}

impl Drop for TLSFRootChunk {
    fn drop(&mut self) {}
}
