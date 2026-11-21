//! Opening MIX archives from disk, and archives nested inside them.

use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;

use ra2_formats::mix::{self, Entry, Index};

/// Bytes that can be read at any offset.
pub trait Source: Send + Sync + fmt::Debug {
    fn len(&self) -> u64;
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A file on disk, read with positioned reads so it can be shared.
#[derive(Debug)]
pub struct FileSource {
    file: File,
    len: u64,
}

impl FileSource {
    pub fn open(path: &Path) -> io::Result<FileSource> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(FileSource { file, len })
    }
}

impl Source for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    #[cfg(unix)]
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        std::os::unix::fs::FileExt::read_exact_at(&self.file, buf, offset)
    }

    #[cfg(windows)]
    fn read_exact_at(&self, mut offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.file.seek_read(buf, offset)? {
                0 => return Err(io::ErrorKind::UnexpectedEof.into()),
                n => {
                    buf = &mut buf[n..];
                    offset += n as u64;
                }
            }
        }
        Ok(())
    }
}

/// Bytes held in memory.
#[derive(Debug, Clone)]
pub struct MemorySource(pub Arc<[u8]>);

impl Source for MemorySource {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let start = usize::try_from(offset).map_err(|_| io::ErrorKind::UnexpectedEof)?;
        let src = self
            .0
            .get(start..start + buf.len())
            .ok_or(io::ErrorKind::UnexpectedEof)?;
        buf.copy_from_slice(src);
        Ok(())
    }
}

/// A range of another source: a file inside an archive.
#[derive(Debug, Clone)]
pub struct Window {
    inner: Arc<dyn Source>,
    start: u64,
    len: u64,
}

impl Window {
    pub fn new(inner: Arc<dyn Source>, start: u64, len: u64) -> io::Result<Window> {
        if start.checked_add(len).is_none_or(|end| end > inner.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "window past the end of its source",
            ));
        }
        Ok(Window { inner, start, len })
    }
}

impl Source for Window {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        if offset
            .checked_add(buf.len() as u64)
            .is_none_or(|end| end > self.len)
        {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.read_exact_at(self.start + offset, buf)
    }
}

/// Read a whole source into memory.
pub fn read_all(src: &dyn Source) -> io::Result<Vec<u8>> {
    let len = usize::try_from(src.len()).map_err(|_| io::Error::other("too large"))?;
    let mut buf = vec![0; len];
    src.read_exact_at(0, &mut buf)?;
    Ok(buf)
}

/// An open MIX archive.
#[derive(Debug, Clone)]
pub struct Archive {
    source: Arc<dyn Source>,
    index: Arc<Index>,
}

fn invalid(e: impl fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

impl Archive {
    /// Open an archive file.
    pub fn open(path: &Path) -> io::Result<Archive> {
        Archive::from_source(Arc::new(FileSource::open(path)?))
    }

    /// Open an archive from any source, reading only its index.
    pub fn from_source(source: Arc<dyn Source>) -> io::Result<Archive> {
        let mut want = usize::try_from(source.len().min(Index::MIN_PREFIX as u64)).expect("small");
        loop {
            let mut prefix = vec![0; want];
            source.read_exact_at(0, &mut prefix)?;
            match Index::parse(&prefix) {
                Ok(index) => {
                    index.check_bounds(source.len()).map_err(invalid)?;
                    return Ok(Archive {
                        source,
                        index: Arc::new(index),
                    });
                }
                Err(mix::Error::NeedMore(n)) if n > want && (n as u64) <= source.len() => want = n,
                Err(mix::Error::NeedMore(_)) => return Err(invalid("archive is truncated")),
                Err(e) => return Err(invalid(e)),
            }
        }
    }

    pub fn index(&self) -> &Index {
        &self.index
    }

    pub fn source(&self) -> &Arc<dyn Source> {
        &self.source
    }

    /// A file in the archive, by name.
    pub fn find(&self, name: &str) -> Option<Entry> {
        self.index.find(name)
    }

    /// The bytes of an entry, as a source that reads them on demand.
    pub fn entry_source(&self, entry: Entry) -> io::Result<Window> {
        let (start, len) = self.index.span(entry);
        Window::new(self.source.clone(), start, len)
    }

    /// Read an entry into memory.
    pub fn read(&self, entry: Entry) -> io::Result<Vec<u8>> {
        read_all(&self.entry_source(entry)?)
    }

    /// An entry by its ID, for files whose names aren't known.
    pub fn find_id(&self, id: u32) -> Option<Entry> {
        self.index.get(id)
    }

    /// Read a file by name, if the archive has it.
    pub fn read_file(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        self.find(name).map(|e| self.read(e)).transpose()
    }

    /// Open an archive stored inside this one.
    pub fn open_nested(&self, entry: Entry) -> io::Result<Archive> {
        Archive::from_source(Arc::new(self.entry_source(entry)?))
    }

    /// Names from the archive's XCC database, if it has one.
    pub fn local_names(&self) -> Vec<String> {
        self.read_file(mix::LOCAL_DATABASE)
            .ok()
            .flatten()
            .and_then(|d| mix::local_database_names(&d))
            .unwrap_or_default()
    }

    /// Verify the SHA-1 after the body, for archives that have one.
    /// `None` if the archive carries no checksum.
    pub fn verify_checksum(&self) -> io::Result<Option<bool>> {
        use sha1::{Digest, Sha1};
        if !self.index.has_checksum() {
            return Ok(None);
        }
        let mut hasher = Sha1::new();
        let mut buf = vec![0; 1 << 20];
        let (start, end) = (
            self.index.body_offset,
            self.index.body_offset + u64::from(self.index.body_size),
        );
        let mut at = start;
        while at < end {
            let n = usize::try_from((end - at).min(buf.len() as u64)).expect("bounded by buf");
            self.source.read_exact_at(at, &mut buf[..n])?;
            hasher.update(&buf[..n]);
            at += n as u64;
        }
        let mut stored = [0u8; 20];
        self.source.read_exact_at(end, &mut stored)?;
        Ok(Some(hasher.finalize()[..] == stored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2_formats::mix::mix_id;

    /// A plain extended archive (flags 0), written by our own encoder.
    fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        let mut entries = Vec::new();
        for (name, data) in files {
            entries.push((
                mix_id(name),
                u32::try_from(body.len()).unwrap(),
                u32::try_from(data.len()).unwrap(),
            ));
            body.extend_from_slice(data);
        }
        let mut out = 0u32.to_le_bytes().to_vec();
        out.extend_from_slice(&u16::try_from(files.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
        for (id, off, size) in entries {
            for v in [id, off, size] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn nested_archives() {
        let inner = build(&[("rulesmd.ini", b"[General]\n")]);
        let outer = build(&[("readme.txt", b"hi"), ("localmd.mix", &inner)]);
        let a = Archive::from_source(Arc::new(MemorySource(outer.into()))).unwrap();
        assert_eq!(a.read_file("README.TXT").unwrap().unwrap(), b"hi");
        let nested = a.open_nested(a.find("localmd.mix").unwrap()).unwrap();
        assert_eq!(
            nested.read_file("rulesmd.ini").unwrap().unwrap(),
            b"[General]\n"
        );
        assert!(nested.read_file("artmd.ini").unwrap().is_none());
        assert_eq!(nested.verify_checksum().unwrap(), None);
    }

    #[test]
    fn truncated_archive_is_an_error() {
        let file = build(&[("a", b"0123456789")]);
        let short = &file[..file.len() - 4];
        assert!(Archive::from_source(Arc::new(MemorySource(short.into()))).is_err());
        assert!(Archive::from_source(Arc::new(MemorySource(file[..9].into()))).is_err());
    }

    #[test]
    fn window_bounds() {
        let src: Arc<dyn Source> = Arc::new(MemorySource(b"abcdef".to_vec().into()));
        let w = Window::new(src.clone(), 2, 3).unwrap();
        assert_eq!(read_all(&w).unwrap(), b"cde");
        let mut b = [0; 2];
        assert!(w.read_exact_at(2, &mut b).is_err());
        assert!(Window::new(src, 4, 3).is_err());
    }
}
