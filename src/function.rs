#![allow(dead_code)]
use super::consts::*;

/// Calculate most significant bit of value.
///
/// If given value is 0, function is failed and returned empty value.
///
/// # Arguments
///
/// * 'value' - target value to calculate.
///
#[inline]
pub fn calculate_msb(value: usize) -> Option<usize> {
    if value == 0 {
        None
    } else {
        let offset = {
            let mut offset = 0;
            let mut value = value;
            while value > 0xFF {
                offset += 8;
                value >>= 8;
            }
            offset
        };
        Some(INDEX_TABLE[value >> offset] as usize + offset)
    }
}

/// Calculate least significant bit of given value.
///
/// If given value is 0, function is failed and return empty value.
///
/// # Arguments
///
/// * 'value' - target value to calculate.
///
#[inline]
pub fn calculate_lsb(value: usize) -> Option<usize> {
    let value = value & (!value).overflowing_add(1).0;
    let offset = {
        let mut value = value;
        let mut offset = 0;
        while value > 0xFF {
            offset += 8;
            value >>= 8;
        }
        offset
    };
    Some(INDEX_TABLE[value >> offset] as usize + offset)
}

/// Calculate mapping indices that represents where to insert block in array.
///
/// # Arguments
///
/// * 'block_size' - Block size target to calculate.
pub fn calculate_mapping_indices(block_size: usize) -> (usize, usize) {
    if block_size < SMALL_BLOCK_SIZE {
        // Second index separation bytes.
        const FRAGMENT: usize = SMALL_BLOCK_SIZE / SECOND_INDEX_MAX;
        (0, block_size / FRAGMENT)
    } else {
        // * Example
        // If block_size is 128, first will be 7.
        // And after more calculation, (7 - 5) = 2, (128 >> 2) = 32, second will be 0.
        // So, (1, 0).
        // In case of 192, (1, 16)...
        //
        // * Fragmentation
        // [128, 256) => 4 Bytes * 32. (1, x)
        // [256, 512) => 8 Bytes * 32. (2, x)
        // ...
        let first = calculate_msb(block_size).unwrap();
        let second = (block_size >> (first - SECOND_INDEX_LOG2_MAX)) - SECOND_INDEX_MAX;
        (first - FIRST_INDEX_OFFSET, second)
    }
}

/// Calculate actual memory allocation size of given size value.
///
/// # Arguments
///
/// * 'size' - Requested allocation size.
#[inline(always)]
pub fn calculate_allocation_size(size: usize) -> usize {
    round_up_block(std::cmp::max(size, MINIMUM_BLOCK_SIZE))
}

///
///
///
pub fn calculate_allocation_searching_size(size: usize) -> usize {
    let mut size = calculate_allocation_size(size);
    if size < SMALL_BLOCK_SIZE {
        size
    } else {
        let t = (1 << (calculate_msb(size).unwrap() - SECOND_INDEX_LOG2_MAX)) - 1;
        size += t;
        size &= !t;
        size
    }
}

/// Round up to 'BLOCK_ALIGNOF'.
///
/// # Arguments
///
/// * 'value' - Value to round up.
///
#[inline(always)]
pub const fn round_up_block(value: usize) -> usize {
    const MASK: usize = BLOCK_ALIGNOF - 1;
    (value + MASK) & !MASK
}

/// Round down to 'BLOCK_ALIGNOF'.
///
/// # Arguments
///
/// * 'value' - Value to round down.
///
#[inline(always)]
pub const fn round_down_block(value: usize) -> usize {
    const MASK: usize = BLOCK_ALIGNOF - 1;
    value & !MASK
}

/// Check given value is aligned to 'BLOCK_ALIGNOF'.
///
/// # Arguments
///
/// * 'value' - Value to check.
///
/// # Examples
///
#[inline(always)]
pub const fn is_aligned(value: usize) -> bool {
    (value & (BLOCK_ALIGNOF - 1)) == 0
}

/// Calculate index to insert into freed block item map.
///
/// # Arguments
///
/// * 'mapping_indices' - first and second level index to calculate.
#[inline(always)]
pub const fn calculate_index(mapping_indices: (usize, usize)) -> usize {
    let (first, second) = mapping_indices;
    first * SECOND_INDEX_MAX + second
}

///
///
#[inline(always)]
pub const fn gigabytes_of(size: usize) -> usize {
    size * 1024 * 1024 * 1024
}

///
///
#[inline(always)]
pub const fn megabytes_of(size: usize) -> usize {
    size * 1024 * 1024
}

///
///
#[inline(always)]
pub const fn kilobytes_of(size: usize) -> usize {
    size * 1024
}

///
///
///
///
///
pub fn next_chunk_size(total: usize, last_chunk_size: usize, size: usize) -> usize {
    const INIT_CHUNK_SIZE: usize = megabytes_of(2);
    const INIT_EXPANDED_ALIGNMENT: usize = megabytes_of(8);

    // Get aligned nearset power of 2 size.
    let aligned_size = match size {
        s if size > 0 => 0x01 << (calculate_msb(s).unwrap() + 1),
        _ => 1024,
    };

    if total == 0 {
        if aligned_size <= (INIT_CHUNK_SIZE >> 2) {
            INIT_CHUNK_SIZE
        } else {
            const ALIGNMENT_MIN1: usize = INIT_EXPANDED_ALIGNMENT - 1;
            (aligned_size * 4usize + ALIGNMENT_MIN1) & !ALIGNMENT_MIN1
        }
    } else {
        let expected = last_chunk_size << 1;
        if aligned_size * 4usize <= expected {
            expected
        } else {
            let min1 = last_chunk_size - 1;
            (aligned_size * 4usize + min1) & !min1
        }
    }
}
