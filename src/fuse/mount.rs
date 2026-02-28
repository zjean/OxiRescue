use crate::blob::BlobStore;
use crate::db::schema::{FileEntry, Folder, MetadataSource};
use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyOpen,
    Request,
};
use libc::{EBADF, EISDIR, ENOENT, EROFS};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TTL: Duration = Duration::from_secs(60);

enum InodeKind {
    Root,
    MetaDir,
    MetaFile { content: Vec<u8> },
    UserDir { user_id: String },
    VirtualFolder { folder: Folder },
    VirtualFile { file: FileEntry },
}

struct InodeEntry {
    kind: InodeKind,
    children: HashMap<String, u64>,
}

pub struct OxiFs {
    blobs: BlobStore,
    inodes: HashMap<u64, InodeEntry>,
    next_inode: u64,
}

impl OxiFs {
    pub fn build(meta: &dyn MetadataSource, blobs: BlobStore) -> anyhow::Result<Self> {
        let mut fs = OxiFs {
            blobs,
            inodes: HashMap::new(),
            next_inode: 4, // 1=root, 2=.meta, 3=stats.json; user entries start at 4
        };

        // Build stats.json content
        let stats = meta.stats()?;
        let dedup_savings_percent = if stats.logical_bytes > 0 {
            let saved = stats.logical_bytes.saturating_sub(stats.physical_bytes);
            (saved as f64 / stats.logical_bytes as f64 * 100.0) as u64
        } else {
            0
        };
        let stats_json = format!(
            "{{\n  \"user_count\": {},\n  \"file_count\": {},\n  \"folder_count\": {},\n  \"unique_blobs\": {},\n  \"logical_bytes\": {},\n  \"physical_bytes\": {},\n  \"dedup_savings_percent\": {}\n}}\n",
            stats.user_count,
            stats.file_count,
            stats.folder_count,
            stats.unique_blobs,
            stats.logical_bytes,
            stats.physical_bytes,
            dedup_savings_percent,
        );
        let stats_content = stats_json.into_bytes();

        // Inode 3: stats.json MetaFile
        fs.inodes.insert(
            3,
            InodeEntry {
                kind: InodeKind::MetaFile {
                    content: stats_content,
                },
                children: HashMap::new(),
            },
        );

        // Inode 2: .meta directory with stats.json child
        let mut meta_children = HashMap::new();
        meta_children.insert("stats.json".to_string(), 3u64);
        fs.inodes.insert(
            2,
            InodeEntry {
                kind: InodeKind::MetaDir,
                children: meta_children,
            },
        );

        // Inode 1: root directory — children added below
        let mut root_children = HashMap::new();
        root_children.insert(".meta".to_string(), 2u64);

        // Build per-user subtrees
        let users = meta.list_users()?;
        for user in &users {
            let user_ino = fs.alloc_inode();
            root_children.insert(user.username.clone(), user_ino);

            let mut user_children = HashMap::new();

            // Root folders for this user
            let root_folders = meta.get_root_folders(&user.id)?;
            for folder in root_folders {
                let folder_ino = fs.alloc_inode();
                user_children.insert(folder.name.clone(), folder_ino);
                fs.build_folder(meta, folder_ino, folder)?;
            }

            // Root-level files (no folder)
            let root_files = meta.list_files_in_folder(&user.id, None)?;
            for file in root_files {
                let file_ino = fs.alloc_inode();
                user_children.insert(file.name.clone(), file_ino);
                fs.inodes.insert(
                    file_ino,
                    InodeEntry {
                        kind: InodeKind::VirtualFile { file },
                        children: HashMap::new(),
                    },
                );
            }

            fs.inodes.insert(
                user_ino,
                InodeEntry {
                    kind: InodeKind::UserDir {
                        user_id: user.id.clone(),
                    },
                    children: user_children,
                },
            );
        }

        // Insert root inode
        fs.inodes.insert(
            1,
            InodeEntry {
                kind: InodeKind::Root,
                children: root_children,
            },
        );

        Ok(fs)
    }

    fn alloc_inode(&mut self) -> u64 {
        let ino = self.next_inode;
        self.next_inode += 1;
        ino
    }

    /// Recursively build the inode tree for a folder (already allocated at `folder_ino`).
    fn build_folder(
        &mut self,
        meta: &dyn MetadataSource,
        folder_ino: u64,
        folder: Folder,
    ) -> anyhow::Result<()> {
        let mut children: HashMap<String, u64> = HashMap::new();

        // Files directly in this folder
        let files = meta.list_files_in_folder(&folder.user_id, Some(&folder.id))?;
        for file in files {
            let file_ino = self.alloc_inode();
            children.insert(file.name.clone(), file_ino);
            self.inodes.insert(
                file_ino,
                InodeEntry {
                    kind: InodeKind::VirtualFile { file },
                    children: HashMap::new(),
                },
            );
        }

        // Sub-folders
        let subfolders = meta.get_subfolders(&folder.id)?;
        for sub in subfolders {
            let sub_ino = self.alloc_inode();
            children.insert(sub.name.clone(), sub_ino);
            self.build_folder(meta, sub_ino, sub)?;
        }

        self.inodes.insert(
            folder_ino,
            InodeEntry {
                kind: InodeKind::VirtualFolder { folder },
                children,
            },
        );

        Ok(())
    }

    fn make_attr(&self, ino: u64, entry: &InodeEntry) -> FileAttr {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let epoch = UNIX_EPOCH;

        match &entry.kind {
            InodeKind::Root
            | InodeKind::MetaDir
            | InodeKind::UserDir { .. }
            | InodeKind::VirtualFolder { .. } => FileAttr {
                ino,
                size: 0,
                blocks: 0,
                atime: epoch,
                mtime: epoch,
                ctime: epoch,
                crtime: epoch,
                kind: FileType::Directory,
                perm: 0o555,
                nlink: 2,
                uid,
                gid,
                rdev: 0,
                flags: 0,
                blksize: 512,
            },
            InodeKind::VirtualFile { file } => {
                let size = file.size;
                let blocks = (size + 511) / 512;
                let mtime = parse_timestamp(file.updated_at.as_deref())
                    .or_else(|| parse_timestamp(file.created_at.as_deref()))
                    .unwrap_or(epoch);
                let ctime = parse_timestamp(file.created_at.as_deref()).unwrap_or(epoch);
                FileAttr {
                    ino,
                    size,
                    blocks,
                    atime: mtime,
                    mtime,
                    ctime,
                    crtime: ctime,
                    kind: FileType::RegularFile,
                    perm: 0o444,
                    nlink: 1,
                    uid,
                    gid,
                    rdev: 0,
                    flags: 0,
                    blksize: 512,
                }
            }
            InodeKind::MetaFile { content } => {
                let size = content.len() as u64;
                let blocks = (size + 511) / 512;
                FileAttr {
                    ino,
                    size,
                    blocks,
                    atime: epoch,
                    mtime: epoch,
                    ctime: epoch,
                    crtime: epoch,
                    kind: FileType::RegularFile,
                    perm: 0o444,
                    nlink: 1,
                    uid,
                    gid,
                    rdev: 0,
                    flags: 0,
                    blksize: 512,
                }
            }
        }
    }
}

fn parse_timestamp(s: Option<&str>) -> Option<SystemTime> {
    let s = s?;
    // Attempt to parse ISO 8601 / RFC 3339 formatted timestamps like "2023-04-01T12:00:00Z"
    // We do a simple manual parse to avoid pulling in chrono just for this.
    // Format: YYYY-MM-DDTHH:MM:SS[.nnn][Z|+HH:MM]
    let s = s.trim_end_matches('Z');
    let s = if let Some(pos) = s.rfind('+') {
        // remove timezone offset
        &s[..pos]
    } else {
        s
    };
    // Remove fractional seconds
    let s = if let Some(pos) = s.find('.') {
        &s[..pos]
    } else {
        s
    };
    // Expected: YYYY-MM-DDTHH:MM:SS
    let parts: Vec<&str> = s.splitn(2, 'T').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<u32> = parts[0].split('-').filter_map(|p| p.parse().ok()).collect();
    let time_parts: Vec<u32> = parts[1].split(':').filter_map(|p| p.parse().ok()).collect();
    if date_parts.len() != 3 || time_parts.len() != 3 {
        return None;
    }
    let (year, month, day) = (date_parts[0] as i64, date_parts[1], date_parts[2]);
    let (hour, min, sec) = (
        time_parts[0] as i64,
        time_parts[1] as i64,
        time_parts[2] as i64,
    );

    // Compute seconds since Unix epoch (1970-01-01T00:00:00Z)
    // Days since epoch using Gregorian calendar formula
    let year_adj = year - 1;
    let days = 365 * year_adj + year_adj / 4 - year_adj / 100 + year_adj / 400;
    let month_days: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_offset: i64 = month_days[..(month as usize - 1)].iter().sum::<i64>()
        + if is_leap && month > 2 { 1 } else { 0 };
    let total_days = days + month_offset + (day as i64 - 1) - 719162; // 719162 = days from year 0 to 1970-01-01
    let total_secs = total_days * 86400 + hour * 3600 + min * 60 + sec;
    if total_secs < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(total_secs as u64))
}

impl Filesystem for OxiFs {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                reply.error(ENOENT);
                return;
            }
        };
        let child_ino = match self.inodes.get(&parent) {
            Some(entry) => match entry.children.get(name_str) {
                Some(&ino) => ino,
                None => {
                    reply.error(ENOENT);
                    return;
                }
            },
            None => {
                reply.error(ENOENT);
                return;
            }
        };
        match self.inodes.get(&child_ino) {
            Some(child_entry) => {
                let attr = self.make_attr(child_ino, child_entry);
                reply.entry(&TTL, &attr, 0);
            }
            None => reply.error(ENOENT),
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        match self.inodes.get(&ino) {
            Some(entry) => {
                let attr = self.make_attr(ino, entry);
                reply.attr(&TTL, &attr);
            }
            None => reply.error(ENOENT),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let entry = match self.inodes.get(&ino) {
            Some(e) => e,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Build a stable sorted list of entries: ".", "..", then children
        let mut entries: Vec<(u64, FileType, String)> = Vec::new();
        entries.push((ino, FileType::Directory, ".".to_string()));
        entries.push((1, FileType::Directory, "..".to_string()));

        let children: Vec<(String, u64)> = entry
            .children
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        // Determine child FileType from their inode entries
        // We need to collect first because we can't borrow self twice
        let child_kinds: Vec<(String, u64, FileType)> = children
            .into_iter()
            .filter_map(|(name, child_ino)| {
                self.inodes.get(&child_ino).map(|child_entry| {
                    let ft = match &child_entry.kind {
                        InodeKind::VirtualFile { .. } | InodeKind::MetaFile { .. } => {
                            FileType::RegularFile
                        }
                        _ => FileType::Directory,
                    };
                    (name, child_ino, ft)
                })
            })
            .collect();

        for (name, child_ino, ft) in child_kinds {
            entries.push((child_ino, ft, name));
        }

        // Sort for deterministic ordering (after "." and "..")
        entries[2..].sort_by(|a, b| a.2.cmp(&b.2));

        for (i, (child_ino, ft, name)) in entries.into_iter().enumerate().skip(offset as usize) {
            // offset+1 as next offset
            let full = reply.add(child_ino, (i + 1) as i64, ft, &name);
            if full {
                break;
            }
        }

        reply.ok();
    }

    fn open(&mut self, _req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
        // Only allow read-only opens
        let access_mode = flags & libc::O_ACCMODE;
        if access_mode != libc::O_RDONLY {
            reply.error(EROFS);
            return;
        }
        match self.inodes.get(&ino) {
            Some(entry) => match &entry.kind {
                InodeKind::VirtualFile { .. } | InodeKind::MetaFile { .. } => {
                    reply.opened(0, 0);
                }
                _ => {
                    reply.error(EISDIR);
                }
            },
            None => reply.error(ENOENT),
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let entry = match self.inodes.get(&ino) {
            Some(e) => e,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        match &entry.kind {
            InodeKind::MetaFile { content } => {
                let start = offset as usize;
                if start >= content.len() {
                    reply.data(&[]);
                    return;
                }
                let end = (start + size as usize).min(content.len());
                reply.data(&content[start..end]);
            }
            InodeKind::VirtualFile { file } => {
                let hash = file.blob_hash.clone();
                match self.blobs.read_blob(&hash) {
                    Ok(data) => {
                        let start = offset as usize;
                        if start >= data.len() {
                            reply.data(&[]);
                            return;
                        }
                        let end = (start + size as usize).min(data.len());
                        reply.data(&data[start..end]);
                    }
                    Err(_) => {
                        reply.error(libc::EIO);
                    }
                }
            }
            InodeKind::Root
            | InodeKind::MetaDir
            | InodeKind::UserDir { .. }
            | InodeKind::VirtualFolder { .. } => {
                reply.error(EISDIR);
            }
        }
    }

    // All write operations return EROFS (read-only filesystem)

    fn write(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        reply.error(EROFS);
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        reply.error(EROFS);
    }

    fn create(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        reply.error(EROFS);
    }

    fn unlink(&mut self, _req: &Request, _parent: u64, _name: &OsStr, reply: fuser::ReplyEmpty) {
        reply.error(EROFS);
    }

    fn rmdir(&mut self, _req: &Request, _parent: u64, _name: &OsStr, reply: fuser::ReplyEmpty) {
        reply.error(EROFS);
    }

    fn rename(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _newparent: u64,
        _newname: &OsStr,
        _flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(EROFS);
    }

    fn setattr(
        &mut self,
        _req: &Request,
        _ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        _size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        reply.error(EROFS);
    }

    fn symlink(
        &mut self,
        _req: &Request,
        _parent: u64,
        _link_name: &OsStr,
        _target: &Path,
        reply: ReplyEntry,
    ) {
        reply.error(EROFS);
    }

    fn link(
        &mut self,
        _req: &Request,
        _ino: u64,
        _newparent: u64,
        _newname: &OsStr,
        reply: ReplyEntry,
    ) {
        reply.error(EROFS);
    }

    fn mknod(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        reply.error(EROFS);
    }

    fn fallocate(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _length: i64,
        _mode: i32,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(EROFS);
    }

    fn copy_file_range(
        &mut self,
        _req: &Request,
        _ino_in: u64,
        _fh_in: u64,
        _offset_in: i64,
        _ino_out: u64,
        _fh_out: u64,
        _offset_out: i64,
        _len: u64,
        _flags: u32,
        reply: fuser::ReplyWrite,
    ) {
        reply.error(EROFS);
    }

    fn release(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(EBADF);
    }
}

pub fn mount_filesystem(
    meta: Box<dyn MetadataSource>,
    blobs: BlobStore,
    mountpoint: &Path,
) -> anyhow::Result<()> {
    let fs = OxiFs::build(meta.as_ref(), blobs)?;
    println!("Mounting OxiCloud filesystem at {}", mountpoint.display());
    println!("Press Ctrl+C to unmount");
    let options = vec![
        fuser::MountOption::RO,
        fuser::MountOption::FSName(String::from("oxirescue")),
    ];
    fuser::mount2(fs, mountpoint, &options)?;
    Ok(())
}
