use tokio::fs::File;

pub struct ActiveSegment {
    pub id: u32,
    pub file: File,
    pub file_name: String,
    pub current_size: u64,
}
