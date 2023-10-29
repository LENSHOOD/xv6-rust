use std::fs::File;
use std::mem;
use std::mem::size_of;
use std::ops::Index;
use crate::deps::{BSIZE, DINode, Dirent, FSMAGIC, FSSIZE, IPB, LOGSIZE, ROOTINO, SuperBlock};
use clap::{arg, Parser};
use crate::deps::FileType::{T_DIR, T_FILE};

mod deps;
const NINODES:usize = 200;

// Disk layout:
// [ boot block | sb block | log | inode blocks | free bit map | data blocks ]

const NBITMAP: usize = FSSIZE/(BSIZE * 8) + 1;
const NINODEBLOCKS: usize = NINODES / IPB + 1;
const NLOG: usize = LOGSIZE;
static mut NMETA: usize = 0;    // Number of meta blocks (boot, sb, nlog, inode, bitmap)
static mut NBLOCKS: usize = 0;  // Number of data blocks

static mut FSFD: usize = 0;
static mut sb: SuperBlock = SuperBlock {
    magic: 0,
    size: 0,
    nblocks: 0,
    ninodes: 0,
    nlog: 0,
    logstart: 0,
    inodestart: 0,
    bmapstart: 0,
};
static mut ZEROES: [u8; BSIZE] = [0; BSIZE];
const FREEINODE: usize = 1;
static mut FREEBLOCK: usize = 0;

#[derive(Parser, Debug)]
struct Args {
    /// Name of the output img file
    #[arg(short, long)]
    output_name: String,

    /// Files that you want to be contained in the img
    #[arg(short, long)]
    files: Option<Vec<String>>,
}
fn main() {
    let i: i32;
    let cc: i32;
    let fd: i32;

    let rootino: u32;
    let inum: u32;
    let off: u32;

    let mut de: Dirent;
    let buf: [u8; BSIZE];
    let mut din: DINode;

    assert_eq!(size_of::<u32>(), 4);
    assert_eq!(BSIZE % size_of::<DINode>(), 0);
    assert_eq!((BSIZE % size_of::<Dirent>()), 0);

    let args: Args = Args::parse();

    let img_file = File::options().read(true).write(true).create(true).truncate(true).open(args.output_name)?;

    // 1 fs block = 1 disk sector
    NMETA = 2 + NLOG + NINODEBLOCKS + NBITMAP;
    NBLOCKS = FSSIZE - NMETA;

    sb.magic = FSMAGIC;
    sb.size = xint(FSSIZE);
    sb.nblocks = xint(NBLOCKS);
    sb.ninodes = xint(NINODES);
    sb.nlog = xint(NLOG);
    sb.logstart = xint(2);
    sb.inodestart = xint(2+NLOG);
    sb.bmapstart = xint(2+NLOG+NINODEBLOCKS);

    println!("nmeta {} (boot, super, log blocks {} inode blocks {}, bitmap blocks {}) blocks %d total {}",
           NMETA, NLOG, NINODEBLOCKS, NBITMAP, NBLOCKS, FSSIZE);

    FREEBLOCK = NMETA;     // the first free block that we can allocate

    for i in 0..FSSIZE {
        wsect(i, ZEROES);
    }

    memset(buf, 0, sizeof(buf));
    memmove(buf, &sb, sizeof(sb));
    wsect(1, buf);

    rootino = ialloc(T_DIR);
    assert_eq!(rootino, ROOTINO);

    bzero(&de, sizeof(de));
    de.inum = xshort(rootino);
    strcpy(de.name, ".");
    iappend(rootino, &de, sizeof(de));

    bzero(&de, sizeof(de));
    de.inum = xshort(rootino);
    strcpy(de.name, "..");
    iappend(rootino, &de, sizeof(de));

    match args.files {
        Some(files) => {
            for file_name in files.iter() {
                // get rid of "user/"
                let short_name = if file_name.starts_with("user/") {
                    &file_name[5..].to_string()
                } else {
                    file_name.to_string()
                };

                assert_eq!(short_name.find("/"), 0);

                let file = File::open(file_name);

                // Skip leading _ in name when writing to file system.
                // The binaries are named _rm, _cat, etc. to keep the
                // build operating system from trying to execute them
                // in place of system binaries like rm and cat.
                if(shortname[0] == '_')
                shortname += 1;

                inum = ialloc(T_FILE);

                bzero(&de, sizeof(de));
                de.inum = xshort(inum);
                strncpy(de.name, shortname, DIRSIZ);
                iappend(rootino, &de, sizeof(de));

                while((cc = read(fd, buf, sizeof(buf))) > 0)
                iappend(inum, buf, cc);

                close(fd);
            }
        }
        _ => {}
    }

    // fix size of root inode dir
    rinode(rootino, &din);
    off = xint(din.size);
    off = ((off/BSIZE) + 1) * BSIZE;
    din.size = xint(off);
    winode(rootino, &din);

    balloc(FREEBLOCK);
}