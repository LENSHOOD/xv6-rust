// Format of an ELF executable file

pub const ELF_MAGIC: u32 = 0x464C457F; // "\x7FELF" in little endian

// File header
#[repr(C)]
pub struct ElfHeader {
    pub(crate) magic: u32, // must equal ELF_MAGIC
    pub(crate) elf: [u8; 12],
    pub(crate) hdr_type: u16,
    pub(crate) machine: u16,
    pub(crate) version: u32,
    pub(crate) entry: u64,
    pub(crate) phoff: u64,
    pub(crate) shoff: u64,
    pub(crate) flags: u32,
    pub(crate) ehsize: u16,
    pub(crate) phentsize: u16,
    pub(crate) phnum: u16,
    pub(crate) shentsize: u16,
    pub(crate) shnum: u16,
    pub(crate) shstrndx: u16,
}

impl ElfHeader {
    pub const fn create() -> Self {
        ElfHeader {
            magic: 0,
            elf: [0; 12],
            hdr_type: 0,
            machine: 0,
            version: 0,
            entry: 0,
            phoff: 0,
            shoff: 0,
            flags: 0,
            ehsize: 0,
            phentsize: 0,
            phnum: 0,
            shentsize: 0,
            shnum: 0,
            shstrndx: 0,
        }
    }
}

// Program section header
#[repr(C)]
pub struct ProgramHeader {
    pub(crate) hdr_type: u32,
    pub(crate) flags: u32,
    pub(crate) off: u64,
    pub(crate) vaddr: u64,
    pub(crate) paddr: u64,
    pub(crate) filesz: u64,
    pub(crate) memsz: u64,
    pub(crate) align: u64,
}

impl ProgramHeader {
    pub const fn create() -> Self {
        ProgramHeader {
            hdr_type: 0,
            flags: 0,
            off: 0,
            vaddr: 0,
            paddr: 0,
            filesz: 0,
            memsz: 0,
            align: 0,
        }
    }
}

// Values for Proghdr type
pub const ELF_PROG_LOAD: u32 = 1;

// Flag bits for Proghdr flags
pub const ELF_PROG_FLAG_EXEC: u32 = 1;
pub const ELF_PROG_FLAG_WRITE: u32 = 2;
pub const ELF_PROG_FLAG_READ: u32 = 4;
