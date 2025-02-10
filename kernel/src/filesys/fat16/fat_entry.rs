#[derive(Debug, Clone, Copy)]
pub struct FatEntry {
    pub cluster: u16,
}

impl FatEntry {
    pub fn is_end_of_chain(&self) -> bool {
        self.cluster >= 0xFFF8
    }

    pub fn is_free(&self) -> bool {
        self.cluster == 0
    }
}
