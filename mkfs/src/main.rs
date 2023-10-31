use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::{cmp, mem};
use std::mem::{size_of, transmute};
use std::ops::Index;
use std::slice::from_raw_parts;
use std::sync::atomic::{AtomicIsize, AtomicU32, AtomicUsize, Ordering};
use crate::deps::{BSIZE, DINode, Dirent, DIRSIZ, FileType, FSMAGIC, FSSIZE, IPB, LOGSIZE, MAXFILE, NDIRECT, NINDIRECT, ROOTINO, SuperBlock};
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
    size: FSSIZE.to_le(),
    nblocks: NBLOCKS.to_le(),
    ninodes: NINODES.to_le(),
    nlog: NLOG.to_le(),
    logstart: 2u32.to_le(),
    inodestart: (2+NLOG).to_le(),
    bmapstart: 2+NLOG+NINODEBLOCKS.to_le(),
};
const ZEROES: [u8; BSIZE] = [0; BSIZE];
static FREEINODE: AtomicU32 = AtomicU32::new(1);

// the first free block that we can allocate
static FREEBLOCK: AtomicU32 = AtomicU32::new(NMETA);

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

    let inum: u32;
    let off: u32;

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

    let mut buf: [u8; BSIZE] = [0; BSIZE];
    let x = unsafe { from_raw_parts(&sb as *const SuperBlock as *const u8, size_of::<SuperBlock>()) };
    buf[x.len()..].clone_from_slice(x);
    wsect(&mut img_file, 1, &buf);

    let rootino = ialloc(&mut img_file, T_DIR);
    assert_eq!(rootino, ROOTINO);

    let mut de = Dirent {
        inum: 0,
        name: [0; DIRSIZ],
    };
    de.inum = rootino as u16.to_le();
    let v = ".".as_bytes();
    de.name[v.len()..].copy_from_slice(v);
    iappend(&mut img_file, rootino, &de, size_of::<Dirent>());

    de.inum = rootino as u16.to_le();
    let v = "..".as_bytes();
    de.name[v.len()..].copy_from_slice(v);
    iappend(&mut img_file, rootino, &de, sizeof(de));

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
                if short_name[0] == '_' {
                    short_name.split(1)[1];
                }

                inum = ialloc(&mut img_file, T_FILE);

                de.inum = inum as u16.to_le();
                let v = short_name.as_bytes();
                de.name[v.len()..].copy_from_slice(v);
                iappend(&mut img_file,rootino, &de, sizeof(de));

                while let cc = img_file.read(&mut buf)? > 0 {
                    iappend(&mut img_file, inum, buf, cc);
                }
            }
        }
        _ => {}
    }

    // fix size of root inode dir
    let mut din = rinode(&mut img_file, rootino);
    off = din.size.to_le();
    off = ((off/BSIZE) + 1) * BSIZE;
    din.size = off.to_le();
    winode(&mut img_file,rootino, din);

    balloc(&mut img_file,FREEBLOCK.load(Ordering::Relaxed) as i32);
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

fn rinode(f: &mut File, inum: u32) -> DINode {
    let bn = IBLOCK!(inum, &sb);

    let mut buf: [u8; BSIZE];
    rsect(f, bn, &mut buf);
    let (head, body, _tail) = unsafe {
        buf[size_of::<DINode>() * (inum % IPB)..size_of::<DINode>() * (inum % IPB + 1)].align_to::<DINode>()
    };

    body[0].clone()
}

fn ialloc(f: &mut File, file_type: FileType) -> u32 {
    let inum = FREEINODE.fetch_add(1, Ordering::Relaxed);

    let din = DINode {
        file_type,
        major: 0,
        minor: 0,
        nlink: 1i16.to_le(),
        size: 0u32.to_le(),
        addrs: [0; NDIRECT + 1],
    };
    winode(f, inum, din);
    return inum;
}

fn balloc(f: &mut File, used: i32) {
    println!("balloc: first {} blocks have been allocated", used);
    assert!(used < (BSIZE*8) as i32);

    let mut buf: [u8; BSIZE] = [0; BSIZE];
    for i in 0..BSIZE {
        buf[i/8] = buf[i/8] | (0x1 << (i%8));
    }

    println!("balloc: write bitmap block at sector {}", &sb.bmapstart);
    wsect(f, (&sb).bmapstart, &buf);
}

fn iappend(f: &mut File, inum: u32, void *xp, n: i32) {
    char *p = (char*)xp;

    let mut din = rinode(f, inum);
    let mut off = din.size.to_le();
    // printf("append inum %d at off %d sz %d\n", inum, off, n);
    let mut n = n;
    let mut indirect: [u32; NINDIRECT] = [0; NINDIRECT];
    while n > 0 {
        let fbn = off / BSIZE;
        assert!(fbn < MAXFILE as u32);
        let x = if fbn < NDIRECT as u32 {
            if xint(din.addrs[fbn]) == 0 {
                din.addrs[fbn] = xint(FREEBLOCK.fetch_add(1, Ordering::Relaxed));
            }
            din.addrs[fbn].to_le()
        } else {
            if xint(din.addrs[NDIRECT]) == 0 {
                din.addrs[NDIRECT] = xint(FREEBLOCK.fetch_add(1, Ordering::Relaxed));
            }
            rsect(f, xint(din.addrs[NDIRECT]), &mut indirect);
            if indirect[fbn - NDIRECT] == 0 {
                indirect[fbn - NDIRECT] = xint(FREEBLOCK.fetch_add(1, Ordering::Relaxed));
                wsect(f, xint(din.addrs[NDIRECT]), (char*)indirect);
            }
            indirect[fbn-NDIRECT].to_le()
        }

        let n1 = cmp::min(n, (fbn + 1) * BSIZE - off);
        let mut buf: [u8; BSIZE] = [0; BSIZE];
        rsect(f, x, &mut buf);
        bcopy(p, buf + off - (fbn * BSIZE), n1);
        wsect(f, x, &buf);
        n -= n1;
        off += n1;
        p += n1;
    }

    din.size = off.to_le();
    winode(f, inum, din);
}
