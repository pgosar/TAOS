pub const PAGE_SIZE: u32 = 4096;
pub const MAX_NUM_CORES: u32 = 16;

pub const BitMapAllocationStatus = enum(u1) {
    FREE,
    ALLOCATED,
};
