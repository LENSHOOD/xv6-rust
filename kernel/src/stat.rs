#[derive(Copy, Clone)]
pub enum FileType {
    NO_TYPE,
    T_DIR, // Directory
    T_FILE, // File
    T_DEVICE, // Device
}

struct Stat {
    dev: i32, // File system's disk device
    ino: u32, // Inode number
    file_type: FileType, // Type of file
    nlink: i16, // Number of links to file
    size: usize, // Size of file in bytes
}
