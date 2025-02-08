use crate::filesys::{BlockDevice, FsError};
use alloc::vec;
use alloc::vec::Vec;
use core::result::Result;

pub struct MemoryBlockDevice {
    blocks: Vec<Vec<u8>>,
    block_size: usize,
}

impl MemoryBlockDevice {
    pub fn new(total_blocks: u64, block_size: usize) -> Self {
        // Initialize all blocks with zeros
        let blocks = (0..total_blocks).map(|_| vec![0; block_size]).collect();

        Self { blocks, block_size }
    }

    fn validate_block(&self, block_num: u64) -> Result<(), FsError> {
        if block_num as usize >= self.blocks.len() {
            return Err(FsError::IOError);
        }
        Ok(())
    }

    fn validate_buffer(&self, buf: &[u8]) -> Result<(), FsError> {
        if buf.len() != self.block_size {
            return Err(FsError::IOError);
        }
        Ok(())
    }
}

impl BlockDevice for MemoryBlockDevice {
    fn read_block(&self, block_num: u64, buf: &mut [u8]) -> Result<(), FsError> {
        self.validate_block(block_num)?;
        self.validate_buffer(buf)?;

        buf.copy_from_slice(&self.blocks[block_num as usize]);
        Ok(())
    }

    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> Result<(), FsError> {
        self.validate_block(block_num)?;
        self.validate_buffer(buf)?;

        self.blocks[block_num as usize].copy_from_slice(buf);
        Ok(())
    }

    fn block_size(&self) -> usize {
        self.block_size
    }

    fn total_blocks(&self) -> u64 {
        self.blocks.len() as u64
    }
}
