use redb::Database;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CACHE: usize = 8 * 1024 * 1024;

// redb 2.6 marks its header writable even for readers. Keep that bookkeeping
// in memory only; all data-page writes and resizing are forbidden. The underlying
// descriptor is O_RDONLY, with an exclusive nonblocking lock for offline use.
#[derive(Debug)]
struct ReadOnlySource {
    file: std::sync::Mutex<File>,
    header: std::sync::Mutex<Option<Vec<u8>>>,
}
impl redb::StorageBackend for ReadOnlySource {
    fn len(&self) -> std::io::Result<u64> {
        Ok(self.file.lock().unwrap().metadata()?.len())
    }
    fn read(&self, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![0; len];
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut bytes)?;
        if let Some(header) = &*self.header.lock().unwrap() {
            if offset < header.len() as u64 {
                let start = offset as usize;
                let n = len.min(header.len() - start);
                bytes[..n].copy_from_slice(&header[start..start + n]);
            }
        }
        Ok(bytes)
    }
    fn write(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        if offset != 0 || data.len() > 4096 {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        *self.header.lock().unwrap() = Some(data.to_vec());
        Ok(())
    }
    fn set_len(&self, _: u64) -> std::io::Result<()> {
        Err(std::io::ErrorKind::PermissionDenied.into())
    }
    fn sync_data(&self, _: bool) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn open_source(path: &Path) -> Result<Database> {
    let file = File::open(path)?;
    file.try_lock()?;
    Ok(Database::builder()
        .set_cache_size(CACHE)
        .set_repair_callback(|session| session.abort())
        .create_with_backend(ReadOnlySource {
            file: std::sync::Mutex::new(file),
            header: Default::default(),
        })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::StorageBackend;
    use std::io::Write;

    #[test]
    fn descriptor_rejects_writes_and_backend_rejects_pages_and_resize() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("descriptor");
        std::fs::write(&path, [0; 8192]).unwrap();
        let source = ReadOnlySource {
            file: std::sync::Mutex::new(File::open(&path).unwrap()),
            header: Default::default(),
        };
        assert!(source
            .file
            .lock()
            .unwrap()
            .write_all(b"cannot write")
            .is_err());
        assert!(source.write(4096, b"page").is_err());
        assert!(source.write(0, &[0; 4097]).is_err());
        assert!(source.set_len(0).is_err());
        source.write(0, b"overlay").unwrap();
        assert_eq!(source.read(0, 7).unwrap(), b"overlay");
        assert_eq!(std::fs::read(path).unwrap(), vec![0; 8192]);
    }
}
