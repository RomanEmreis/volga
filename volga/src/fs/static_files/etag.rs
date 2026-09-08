//! Deriving the `ETag` of a static file, and remembering the one that costs a read.
//!
//! [`ETagSource::Metadata`] is answered from the `stat` the server has already made, so
//! there is nothing here to cache. [`ETagSource::Content`] reads the file, and this module
//! is what keeps that read to once per version instead of once per request.
//!
//! # What an entry is validated against
//!
//! A cache that is validated by metadata inherits whatever that metadata cannot see, which
//! is the very thing [`ETagSource::Content`] exists to escape - so the check here is
//! deliberately finer than the one a tag is derived from in [`ETagSource::Metadata`]:
//!
//! * the **length**, and the **modification time to the nanosecond**. Whole seconds are all
//!   a tag may carry, because a sub-second `mtime` differs between replicas of one build and
//!   the tag has to agree across them - but an entry in this cache is never compared against
//!   anything outside this process, so it is free to use every digit the filesystem reports.
//! * the **file's identity**, where the platform gives one - the inode on Unix. A deploy
//!   that writes a new file and renames it over the old, which is what `rsync`, `tar` and
//!   every atomic deploy script do, moves this even when it restores both the length and the
//!   modification time of what it replaced.
//!
//! What is left is a file rewritten *in place*, keeping its length, its inode and its
//! modification time down to the nanosecond, inside the lifetime of one process. Nothing
//! short of reading the file on every request sees that, and a deploy that restarts the
//! server - a new container, a new binary, a `systemctl restart` - starts from an empty
//! cache regardless.
//!
//! The cache is process-wide because its key is: one path, length, modification time and
//! inode describe the same bytes whichever [`App`](crate::App) asked for them.

use crate::{
    error::Error,
    headers::{ETag, ETagSource},
    utils::lower_hex,
};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fs::Metadata,
    path::{Path, PathBuf},
    sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::SystemTime,
};
use tokio::{fs::File, io::AsyncReadExt};

/// How much of a file is read at a time while it is hashed.
///
/// Hashing a chunk this size takes tens of microseconds, which is short enough to sit
/// between two `await` points without holding up the runtime - so a file of any size is
/// hashed on the runtime rather than handed to [`spawn_blocking`], and the read that
/// dominates it is where it belongs either way.
///
/// [`spawn_blocking`]: tokio::task::spawn_blocking
const CHUNK_SIZE: usize = 64 * 1024;

/// How many entries the live generation of the cache holds before it is retired.
///
/// The cache is bounded because a content root is not: a tree with a file per user would
/// otherwise grow an entry per file and never give one back. Two generations are kept, so
/// the bound on what is held is twice this.
const GENERATION_CAPACITY: usize = 1024;

/// Which version of a file is on disk, as far as a `stat` can tell.
///
/// See the [module documentation](self) for why each part is here and what the three of
/// them together still cannot see.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Version {
    len: u64,
    modified: SystemTime,
    id: Option<u64>,
}

/// A derived tag, and the version of the file it describes.
struct Entry {
    version: Version,
    etag: ETag,
}

/// The cache proper: a live generation that fills up, and the one it displaced.
///
/// Retiring a full generation wholesale is an approximation of evicting the least recently
/// used entry, and it costs no bookkeeping per lookup to maintain - a hit in the retired
/// generation is copied back into the live one, so what is still being asked for survives
/// the next retirement and what is not falls out of the cache one retirement later.
#[derive(Default)]
struct Generations {
    live: HashMap<PathBuf, Entry>,
    retired: HashMap<PathBuf, Entry>,
}

static CACHE: OnceLock<RwLock<Generations>> = OnceLock::new();

impl Version {
    /// Reads the version off a `stat` the caller has already made.
    #[inline]
    fn of(metadata: &Metadata) -> Result<Self, Error> {
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified()?,
            id: file_id(metadata),
        })
    }
}

/// The identity the filesystem gives a file, where the platform exposes one.
#[inline]
fn file_id(metadata: &Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.ino())
    }

    // Windows reports one only through `windows_by_handle`, which is unstable, so there is
    // nothing to read here yet. The length and the modification time still apply.
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

/// Derives the `ETag` of `path` from the source its role is configured with.
#[inline]
pub(super) async fn of(
    path: &Path,
    metadata: &Metadata,
    source: ETagSource,
) -> Result<ETag, Error> {
    match source {
        ETagSource::Metadata => ETag::try_from(metadata),
        ETagSource::Content => from_contents(path, metadata).await,
    }
}

/// Derives the tag from the file's bytes, reading them only when no entry describes the
/// version that is on disk.
#[inline]
async fn from_contents(path: &Path, metadata: &Metadata) -> Result<ETag, Error> {
    let version = Version::of(metadata)?;

    if let Some(etag) = cached(path, version) {
        return Ok(etag);
    }

    // Nothing keeps two requests that arrive on a cold entry from reading the file at the
    // same time. They hash the same bytes and store the same tag, so the race is paid for
    // in a second read and in nothing else - which is cheaper than the machinery that would
    // hold one of them while the other reads.
    let etag = ETag::try_weak(hash_contents(path).await?)?;
    store(path, version, etag.clone());

    Ok(etag)
}

/// Reads the file and returns the hex SHA-1 of its contents.
#[inline]
async fn hash_contents(path: &Path) -> Result<String, Error> {
    let mut file = File::open(path).await?;
    let mut buf = vec![0_u8; CHUNK_SIZE];
    let mut hasher = Sha1::new();

    loop {
        let read = file.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(lower_hex(&hasher.finalize()))
}

/// Returns the tag remembered for this version of the file, if one is.
#[inline]
fn cached(path: &Path, version: Version) -> Option<ETag> {
    let generations = read_lock();

    if let Some(entry) = generations.live.get(path)
        && entry.version == version
    {
        return Some(entry.etag.clone());
    }

    let etag = generations
        .retired
        .get(path)
        .filter(|entry| entry.version == version)
        .map(|entry| entry.etag.clone())?;

    // Still being asked for, so it is copied back into the live generation rather than left
    // to fall out with the rest of the retired one.
    drop(generations);
    store(path, version, etag.clone());

    Some(etag)
}

/// Remembers a derived tag, retiring the live generation first when it is full.
#[inline]
fn store(path: &Path, version: Version, etag: ETag) {
    let mut generations = write_lock();

    if generations.live.len() >= GENERATION_CAPACITY {
        generations.retired = std::mem::take(&mut generations.live);
    }

    generations
        .live
        .insert(path.to_path_buf(), Entry { version, etag });
}

/// Takes the cache for reading, treating a poisoned lock as a readable one.
///
/// Nothing but a `HashMap` lookup happens under this lock, so it is poisoned only by a
/// panic that had nothing to do with what it guards - and the entries behind it stay exactly
/// as valid as they were. Refusing to read them would turn an unrelated panic into a cache
/// that is empty for the rest of the process.
#[inline]
fn read_lock() -> RwLockReadGuard<'static, Generations> {
    CACHE
        .get_or_init(Default::default)
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Takes the cache for writing. See [`read_lock`] for why a poisoned lock is still used.
#[inline]
fn write_lock() -> RwLockWriteGuard<'static, Generations> {
    CACHE
        .get_or_init(Default::default)
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A path no other test writes to, so that the process-wide cache cannot carry a result
    /// from one test into another.
    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("volga-etag-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    fn write(path: &Path, contents: &[u8]) -> Metadata {
        std::fs::write(path, contents).expect("write");
        std::fs::metadata(path).expect("metadata")
    }

    fn version_of(path: &Path) -> Version {
        Version::of(&std::fs::metadata(path).expect("metadata")).expect("version")
    }

    /// Replaces a file the way a deploy does - a new file renamed over the old - restoring
    /// the modification time of what it replaced, so that only the bytes and the inode move.
    fn deploy(path: &Path, contents: &[u8], modified: SystemTime) -> Metadata {
        let staged = path.with_extension("staged");
        std::fs::write(&staged, contents).expect("write");

        std::fs::File::options()
            .write(true)
            .open(&staged)
            .expect("open")
            .set_modified(modified)
            .expect("set modified");

        std::fs::rename(&staged, path).expect("rename");
        std::fs::metadata(path).expect("metadata")
    }

    /// The case reported in #233, with the timing pinned rather than raced: two shells of
    /// the same byte length and the same modification time to the nanosecond.
    #[tokio::test]
    async fn content_tags_differ_for_same_length_files_at_the_same_instant() {
        let path = temp_path("collision.html");

        let before_meta = write(&path, b"<script src=/a1b2c3.js>");
        let before = of(&path, &before_meta, ETagSource::Content).await.unwrap();

        let after_meta = deploy(
            &path,
            b"<script src=/d4e5f6.js>",
            before_meta.modified().unwrap(),
        );
        let after = of(&path, &after_meta, ETagSource::Content).await.unwrap();

        // Nothing a `stat` reports has moved except the inode.
        assert_eq!(before_meta.len(), after_meta.len());
        assert_eq!(
            before_meta.modified().unwrap(),
            after_meta.modified().unwrap()
        );

        assert_ne!(before.as_ref(), after.as_ref());
    }

    /// The same pair through the metadata source, which is where the collision lives - and
    /// which is why the shell is not served from it.
    #[tokio::test]
    async fn metadata_tags_collide_where_content_tags_do_not() {
        let path = temp_path("metadata-collision.html");

        let before_meta = write(&path, b"<script src=/a1b2c3.js>");
        let before = of(&path, &before_meta, ETagSource::Metadata).await.unwrap();

        let after_meta = deploy(
            &path,
            b"<script src=/d4e5f6.js>",
            before_meta.modified().unwrap(),
        );
        let after = of(&path, &after_meta, ETagSource::Metadata).await.unwrap();

        assert_eq!(before.as_ref(), after.as_ref());
    }

    #[tokio::test]
    async fn content_tag_is_weak() {
        let path = temp_path("weak.css");
        let metadata = write(&path, b"body {}");

        let etag = of(&path, &metadata, ETagSource::Content).await.unwrap();

        assert!(etag.is_weak());
    }

    /// The same bytes under two names derive the same tag, which is what makes the tag agree
    /// across replicas of one deployment where a sub-second `mtime` would not.
    #[tokio::test]
    async fn content_tag_follows_the_bytes_rather_than_the_file() {
        let here = temp_path("replica-a.css");
        let there = temp_path("replica-b.css");

        let here_meta = write(&here, b"h1 { color: red }");
        // Written a moment later, so the two disagree on `mtime` the way two replicas do.
        std::thread::sleep(Duration::from_millis(20));
        let there_meta = write(&there, b"h1 { color: red }");

        assert_ne!(
            here_meta.modified().unwrap(),
            there_meta.modified().unwrap()
        );

        let here_tag = of(&here, &here_meta, ETagSource::Content).await.unwrap();
        let there_tag = of(&there, &there_meta, ETagSource::Content).await.unwrap();

        assert_eq!(here_tag.as_ref(), there_tag.as_ref());
    }

    #[tokio::test]
    async fn cache_answers_a_second_request_for_one_version() {
        let path = temp_path("cached.js");
        let metadata = write(&path, b"export default 1");

        let first = of(&path, &metadata, ETagSource::Content).await.unwrap();

        assert!(cached(&path, version_of(&path)).is_some());

        let second = of(&path, &metadata, ETagSource::Content).await.unwrap();
        assert_eq!(first.as_ref(), second.as_ref());
    }

    #[tokio::test]
    async fn cache_entry_is_rejected_once_any_part_of_the_version_moves() {
        let path = temp_path("stale.js");
        let metadata = write(&path, b"export default 1");

        let _ = of(&path, &metadata, ETagSource::Content).await.unwrap();
        let current = version_of(&path);

        for moved in [
            Version {
                len: current.len + 1,
                ..current
            },
            Version {
                modified: current.modified + Duration::from_nanos(1),
                ..current
            },
            Version {
                id: current.id.map(|id| id + 1),
                ..current
            },
        ] {
            // The inode is the one part a platform may not report, and `None` moves nowhere.
            if moved == current {
                continue;
            }
            assert!(cached(&path, moved).is_none());
        }
    }

    #[tokio::test]
    async fn hashing_spans_more_than_one_chunk() {
        let path = temp_path("large.bin");

        // Two chunks and a bit, so the read loop runs more than once and the tail is covered
        // by what it hashes.
        let mut contents = vec![b'a'; CHUNK_SIZE * 2 + 7];
        let first_meta = write(&path, &contents);
        let first = of(&path, &first_meta, ETagSource::Content).await.unwrap();

        // A single byte, in the tail a chunk boundary would be most likely to drop.
        let last = contents.len() - 1;
        contents[last] = b'b';
        let second_meta = deploy(&path, &contents, first_meta.modified().unwrap());
        let second = of(&path, &second_meta, ETagSource::Content).await.unwrap();

        assert_eq!(first_meta.len(), second_meta.len());
        assert_ne!(first.as_ref(), second.as_ref());
    }

    #[tokio::test]
    async fn missing_file_is_an_error_rather_than_a_tag() {
        let path = temp_path("nothing-here.html");
        let metadata = write(&path, b"gone in a moment");
        std::fs::remove_file(&path).expect("remove");

        assert!(of(&path, &metadata, ETagSource::Content).await.is_err());
    }

    /// A full generation is retired rather than grown, and what it held is still answered
    /// from the retired one until the next retirement displaces it.
    #[tokio::test]
    async fn cache_retires_a_full_generation_instead_of_growing() {
        let path = temp_path("retired.js");
        let metadata = write(&path, b"export default 1");
        let version = Version::of(&metadata).unwrap();

        let etag = of(&path, &metadata, ETagSource::Content).await.unwrap();

        for i in 0..GENERATION_CAPACITY {
            store(
                Path::new(&format!("/nowhere/{i}")),
                version,
                ETag::weak(format!("filler-{i}")),
            );
        }

        let generations = read_lock();
        assert!(generations.live.len() <= GENERATION_CAPACITY);
        drop(generations);

        // Displaced out of the live generation, still answered from the retired one.
        assert_eq!(cached(&path, version).unwrap().as_ref(), etag.as_ref());
    }
}
