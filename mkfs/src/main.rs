use crate::deps::FileType::{T_DIR, T_FILE};
use crate::deps::{
    DINode, Dirent, FileType, SuperBlock, BSIZE, DIRSIZ, FSMAGIC, FSSIZE, IPB, LOGSIZE, MAXFILE,
    NDIRECT, NINDIRECT, ROOTINO,
};
use clap::Parser;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::slice::from_raw_parts;
use std::sync::atomic::{AtomicU32, Ordering};
use std::{cmp, io::Result};

mod deps;
const NINODES: u32 = 200;

// Disk layout:
// [ boot block | sb block | log | inode blocks | free bit map | data blocks ]

const NBITMAP: u32 = FSSIZE / (BSIZE as u32 * 8) + 1;
const NINODEBLOCKS: u32 = NINODES / IPB + 1;
const NLOG: u32 = LOGSIZE;

// 1 fs block = 1 disk sector
const NMETA: u32 = 2 + NLOG + NINODEBLOCKS + NBITMAP; // Number of meta blocks (boot, sb, nlog, inode, bitmap)
const NBLOCKS: u32 = FSSIZE - NMETA; // Number of data blocks

const SB: SuperBlock = SuperBlock {
    magic: FSMAGIC,
    size: FSSIZE.to_le(),
    nblocks: NBLOCKS.to_le(),
    ninodes: NINODES.to_le(),
    nlog: NLOG.to_le(),
    logstart: 2u32.to_le(),
    inodestart: (2 + NLOG).to_le(),
    bmapstart: (2 + NLOG + NINODEBLOCKS).to_le(),
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
fn main() -> Result<()> {
    assert_eq!(size_of::<u32>(), 4);
    assert_eq!(BSIZE % size_of::<DINode>(), 0);
    assert_eq!((BSIZE % size_of::<Dirent>()), 0);

    let args: Args = Args::parse();

    let mut img_file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(args.output_name)?;

    println!("nmeta {} (boot, super, log blocks {} inode blocks {}, bitmap blocks {}) blocks {} total {}",
           NMETA, NLOG, NINODEBLOCKS, NBITMAP, NBLOCKS, FSSIZE);

    for i in 0..FSSIZE {
        wsect(&mut img_file, i, &ZEROES)?;
    }

    let x = unsafe {
        from_raw_parts(
            &SB as *const SuperBlock as *const u8,
            size_of::<SuperBlock>(),
        )
    };
    let mut buf: [u8; BSIZE] = [0; BSIZE];
    buf[..x.len()].clone_from_slice(x);
    wsect(&mut img_file, 1, &buf)?;

    let rootino = ialloc(&mut img_file, T_DIR)?;
    assert_eq!(rootino, ROOTINO);

    let mut de = Dirent {
        inum: 0,
        name: [0; DIRSIZ],
    };
    de.inum = (rootino as u16).to_le();
    let v = ".".as_bytes();
    de.name[..v.len()].copy_from_slice(v);
    iappend(&mut img_file, rootino, &de, size_of::<Dirent>() as i32)?;

    de.inum = (rootino as u16).to_le();
    let v = "..".as_bytes();
    de.name[..v.len()].copy_from_slice(v);
    iappend(&mut img_file, rootino, &de, size_of::<Dirent>() as i32)?;

    match args.files {
        Some(files) => {
            for file_name in files.iter() {
                // get rid of "user/"
                let mut short_name = if file_name.starts_with("user/") {
                    file_name[5..].to_string()
                } else {
                    file_name.to_string()
                };

                assert_eq!(short_name.find("/"), None);

                let mut file = File::open(file_name)?;

                // Skip leading _ in name when writing to file system.
                // The binaries are named _rm, _cat, etc. to keep the
                // build operating system from trying to execute them
                // in place of system binaries like rm and cat.
                if let Some(s) = short_name.strip_prefix("_") {
                    short_name = s.to_string();
                }

                let inum = ialloc(&mut img_file, T_FILE)?;

                de.inum = (inum as u16).to_le();
                let v = short_name.as_bytes();
                de.name[..v.len()].copy_from_slice(v);
                iappend(&mut img_file, rootino, &de, size_of::<Dirent>() as i32)?;

                let mut cc = 1;
                while cc > 0 {
                    let mut buf: [u8; BSIZE] = [0; BSIZE];
                    cc = file.read(&mut buf)?;
                    iappend(&mut img_file, inum, &buf, cc as i32)?;
                }
            }
        }
        _ => {}
    }

    // fix size of root inode dir
    let mut din = rinode(&mut img_file, rootino);
    let mut off = din.size.to_le();
    off = (((off as usize / BSIZE) + 1) * BSIZE) as u32;
    din.size = off.to_le();
    winode(&mut img_file, rootino, din)?;

    balloc(&mut img_file, FREEBLOCK.load(Ordering::Relaxed) as i32)?;

    Ok(())
}

fn wsect(f: &mut File, sec: u32, buf: &[u8]) -> Result<()> {
    if f.seek(SeekFrom::Start(sec as u64 * BSIZE as u64))? != sec as u64 * BSIZE as u64 {
        panic!("lseek");
    }
    if f.write(buf)? != BSIZE {
        panic!("write");
    }

    Ok(())
}

fn rsect(f: &mut File, sec: u32, buf: &mut [u8]) -> Result<()> {
    if f.seek(SeekFrom::Start(sec as u64 * BSIZE as u64))? != sec as u64 * BSIZE as u64 {
        panic!("lseek");
    }
    if f.read(buf)? != BSIZE {
        panic!("read");
    }

    Ok(())
}

fn winode(f: &mut File, inum: u32, ip: DINode) -> Result<()> {
    let bn = IBLOCK!(inum, &SB);
    let mut buf: [u8; BSIZE] = [0; BSIZE];
    rsect(f, bn, &mut buf)?;

    let ino_sz = size_of::<DINode>();
    let x = unsafe { from_raw_parts(&ip as *const DINode as *const u8, ino_sz) };
    buf[ino_sz * (inum % IPB) as usize..ino_sz * ((inum + 1) % IPB) as usize].clone_from_slice(x);
    wsect(f, bn, &buf)?;

    Ok(())
}

fn rinode(f: &mut File, inum: u32) -> DINode {
    let bn = IBLOCK!(inum, &SB);

    let mut buf: [u8; BSIZE] = [0; BSIZE];
    rsect(f, bn, &mut buf).unwrap();
    let (_head, body, _tail) = unsafe {
        let ino_sz = size_of::<DINode>();
        buf[ino_sz * (inum % IPB) as usize..ino_sz * ((inum + 1) % IPB) as usize]
            .align_to::<DINode>()
    };

    body[0].clone()
}

fn ialloc(f: &mut File, file_type: FileType) -> Result<u32> {
    let inum = FREEINODE.fetch_add(1, Ordering::Relaxed);

    let din = DINode {
        file_type,
        major: 0,
        minor: 0,
        nlink: 1i16.to_le(),
        size: 0u32.to_le(),
        addrs: [0; NDIRECT + 1],
    };
    winode(f, inum, din)?;
    return Ok(inum);
}

fn balloc(f: &mut File, used: i32) -> Result<()> {
    println!("balloc: first {} blocks have been allocated", used);
    assert!(used < (BSIZE * 8) as i32);

    let mut buf: [u8; BSIZE] = [0; BSIZE];
    for i in 0..BSIZE {
        buf[i / 8] = buf[i / 8] | (0x1 << (i % 8));
    }

    println!("balloc: write bitmap block at sector {}", &SB.bmapstart);
    wsect(f, (&SB).bmapstart, &buf)?;
    Ok(())
}

fn iappend<T>(f: &mut File, inum: u32, xp: &T, n: i32) -> Result<()> {
    let xp = unsafe { from_raw_parts(xp as *const T as *const u8, size_of::<T>()) };

    let mut din = rinode(f, inum);
    let mut off = din.size.to_le();
    // printf("append inum %d at off %d sz %d\n", inum, off, n);
    let mut n = n;
    let mut indirect: [u32; NINDIRECT] = [0; NINDIRECT];
    let mut buf: [u8; BSIZE] = [0; BSIZE];
    let mut pos = 0;
    while n > 0 {
        let fbn = off as usize / BSIZE;
        assert!(fbn < MAXFILE);
        let x = if fbn < NDIRECT {
            if din.addrs[fbn].to_le() == 0 {
                let a = FREEBLOCK.fetch_add(1, Ordering::Relaxed);
                let b = a.to_le();
                din.addrs[fbn] = b;
            }
            let c = din.addrs[fbn];
            let d = c.to_le();
            d
        } else {
            if din.addrs[NDIRECT].to_le() == 0 {
                din.addrs[NDIRECT] = FREEBLOCK.fetch_add(1, Ordering::Relaxed).to_le();
            }
            let mut buf: [u8; NINDIRECT * 4] = unsafe { std::mem::transmute(indirect) };
            rsect(f, din.addrs[NDIRECT].to_le(), &mut buf)?;
            if indirect[fbn - NDIRECT] == 0 {
                indirect[fbn - NDIRECT] = FREEBLOCK.fetch_add(1, Ordering::Relaxed).to_le();
                let mut buf: [u8; NINDIRECT * 4] = unsafe { std::mem::transmute(indirect) };
                wsect(f, din.addrs[NDIRECT].to_le(), &mut buf)?;
            }
            indirect[fbn - NDIRECT].to_le()
        };

        let n1 = cmp::min(n as usize, (fbn + 1) * BSIZE - off as usize);
        rsect(f, x, &mut buf)?;

        let start = off as usize - (fbn * BSIZE);
        buf[start..start + n1].clone_from_slice(&xp[pos..pos + n1]);
        wsect(f, x, &buf)?;
        n -= n1 as i32;
        off += n1 as u32;
        pos += n1;
    }

    din.size = off.to_le();
    winode(f, inum, din)?;

    Ok(())
}
