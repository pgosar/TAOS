use super::constants::*;
use super::*;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DirEntry83 {
    pub name: [u8; 8],
    pub ext: [u8; 3],
    pub attributes: u8,
    pub reserved: [u8; 10],
    pub time: u16,
    pub date: u16,
    pub start_cluster: u16,
    pub file_size: u32,
}

impl DirEntry83 {
    pub fn new_file(name: &str, ext: &str, start_cluster: u16) -> Self {
        let mut entry = Self {
            name: [0x20; 8],
            ext: [0x20; 3],
            attributes: ATTR_ARCHIVE,
            reserved: [0; 10],
            time: 0,
            date: 0,
            start_cluster,
            file_size: 0,
        };

        let name_bytes = name.as_bytes();
        entry.name[..name_bytes.len().min(8)]
            .copy_from_slice(&name_bytes[..name_bytes.len().min(8)]);

        let ext_bytes = ext.as_bytes();
        entry.ext[..ext_bytes.len().min(3)].copy_from_slice(&ext_bytes[..ext_bytes.len().min(3)]);

        entry
    }

    pub fn new_directory(name: &str, start_cluster: u16) -> Self {
        let mut entry = Self::new_file(name, "", start_cluster);
        entry.attributes = ATTR_DIRECTORY;
        entry
    }

    pub fn is_deleted(&self) -> bool {
        self.name[0] == DELETED_ENTRY_MARKER
    }

    pub fn is_free(&self) -> bool {
        self.name[0] == 0x00
    }

    pub fn is_directory(&self) -> bool {
        self.attributes & ATTR_DIRECTORY != 0
    }

    pub fn get_name(&self) -> String {
        let name_end = self.name.iter().position(|&x| x == 0x20).unwrap_or(8);
        let ext_end = self.ext.iter().position(|&x| x == 0x20).unwrap_or(3);

        let name = core::str::from_utf8(&self.name[..name_end]).unwrap_or("");
        let ext = core::str::from_utf8(&self.ext[..ext_end]).unwrap_or("");

        if ext_end > 0 {
            alloc::format!("{}.{}", name, ext)
        } else {
            name.into()
        }
    }
}
