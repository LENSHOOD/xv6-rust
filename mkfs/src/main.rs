use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem;
use std::mem::{size_of, transmute};
use std::ops::Index;
use std::slice::from_raw_parts;
use crate::deps::{BSIZE, DINode, Dirent, FSMAGIC, FSSIZE, IPB, LOGSIZE, ROOTINO, SuperBlock};
use clap::{arg, Parser};
use crate::deps::FileType::{T_DIR, T_FILE};

mod deps;
const NINODES: u32 = 200;

// Disk layout:
// [ boot block | sb block | log | inode blocks | free bit map | data blocks ]

const NBITMAP: u32 = FSSIZE/(BSIZE * 8) + 1;
const NINODEBLOCKS: u32 = NINODES / IPB + 1;
const NLOG: u32 = LOGSIZE;

// 1 fs block = 1 disk sector
const NMETA: u32 = 2 + NLOG + NINODEBLOCKS + NBITMAP;    // Number of meta blocks (boot, sb, nlog, inode, bitmap)
const NBLOCKS: u32 = FSSIZE - NMETA;  // Number of data blocks

const sb: SuperBlock = SuperBlock {
    magic: FSMAGIC,
    size: xint(FSSIZE),
    nblocks: xint(NBLOCKS),
    ninodes: xint(NINODES),
    nlog: xint(NLOG),
    logstart: xint(2),
    inodestart: xint(2+NLOG),
    bmapstart: xint(2+NLOG+NINODEBLOCKS),
};
const ZEROES: [u8; BSIZE] = [0; BSIZE];
const FREEINODE: usize = 1;

// the first free block that we can allocate
const FREEBLOCK: u32 = NMETA;

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
    let cc: i32;
    let fd: i32;

    let rootino: u32;
    let inum: u32;
    let off: u32;

    let mut de: Dirent;
    let mut din: DINode;

    assert_eq!(size_of::<u32>(), 4);
    assert_eq!(BSIZE % size_of::<DINode>(), 0);
    assert_eq!((BSIZE % size_of::<Dirent>()), 0);

    let args: Args = Args::parse();

    let mut img_file =
        File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(args.output_name)?;

    println!("nmeta {} (boot, super, log blocks {} inode blocks {}, bitmap blocks {}) blocks {} total {}",
           NMETA, NLOG, NINODEBLOCKS, NBITMAP, NBLOCKS, FSSIZE);

    for i in 0..FSSIZE {
        wsect(&mut img_file, i, &ZEROES);
    }

    let buf: [u8; BSIZE] = [0; BSIZE];
    let x = unsafe { from_raw_parts(&sb as *const SuperBlock as *const u8, size_of::<SuperBlock>()) };
    buf[x.len()..].clone_from_slice(x);
    wsect(&mut img_file, 1, &buf);

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

// convert to riscv byte order
const fn xshort(x: u16) -> u16 {
    x.to_le()
}

const fn xint(x: u32) -> u32 {
    x.to_le()
}

fn wsect(f: &mut File, sec: u32, buf: &[u8]) {
    if f.seek(SeekFrom::Start((sec * BSIZE) as u64))? != (sec * BSIZE) as u64 {
        panic!("lseek");
    }
    if f.write(buf)? != BSIZE {
        panic!("write");
    }
}

fn rsect(f: &mut File, sec: u32, buf: &mut [u8]) {
    if f.seek(SeekFrom::Start((sec * BSIZE) as u64))? != (sec * BSIZE) as u64 {
        panic!("lseek");
    }
    if f.read(buf)? != BSIZE {
        panic!("read");
    }
}

fn winode(f: &mut File, inum: u32, ip: DINode) {
    let bn = IBLOCK!(inum, &sb);
    let mut buf: [u8; BSIZE];
    rsect(f, bn, &mut buf);

    let x = unsafe { from_raw_parts(&ip as *const DINode as *const u8, size_of::<DINode>()) };
    buf[x.len() * (inum % IPB)..(x.len() + 1) * (inum % IPB)].clone_from_slice(x);
    wsect(f, bn, &buf);
}

fn rinode(f: &mut File, inum: u32, struct dinode *ip) {
    struct dinode *dip;

    let bn = IBLOCK!(inum, &sb);

    let mut buf: [u8; BSIZE];
    rsect(f, bn, &mut buf);
    dip = ((struct dinode*)buf) + (inum % IPB);
    *ip = *dip;
}
