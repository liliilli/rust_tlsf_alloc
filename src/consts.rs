use std::mem;

/// BlockHeader size must be bigger than MINIMUM_BLOCK_SIZE.
pub const MINIMUM_BLOCK_SIZE: usize = 16usize;
pub const BLOCK_ALIGNOF: usize = mem::size_of::<*const u8>() * 2;
/// Small block size that first index of mapping size is always be 0.
pub const SMALL_BLOCK_SIZE: usize = 128usize;

///
pub const FIRST_INDEX_MAX: usize = 36;
pub const FIRST_INDEX_OFFSET: usize = 6;
pub const SECOND_INDEX_LOG2_MAX: usize = 5;
pub const SECOND_INDEX_MAX: usize = 1 << SECOND_INDEX_LOG2_MAX;

///
pub const FIRST_INDEX_REAL: usize = FIRST_INDEX_MAX - FIRST_INDEX_OFFSET;
///
pub const TOTAL_COUNT: usize = FIRST_INDEX_REAL * SECOND_INDEX_MAX;

/// Index table for seaching most significant bit and least significant bit.
pub const INDEX_TABLE: [u16; 256] = [
    0, // Invalue value
    0, // 1
    1, 1, // 2
    2, 2, 2, 2, // 4
    3, 3, 3, 3, 3, 3, 3, 3, // 8
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, // 16
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, // 32
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, // 64
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, // 128
];
