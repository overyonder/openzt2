use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    sync::Arc,
};

#[derive(Clone)]
pub(super) struct PositionalArchiveReader {
    file: Arc<File>,
    position: u64,
    length: u64,
}

impl PositionalArchiveReader {
    pub(super) fn new(file: File) -> io::Result<Self> {
        let length = file.metadata()?.len();
        Ok(Self {
            file: Arc::new(file),
            position: 0,
            length,
        })
    }
}

impl Read for PositionalArchiveReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        #[cfg(unix)]
        let count = std::os::unix::fs::FileExt::read_at(&*self.file, bytes, self.position)?;
        #[cfg(windows)]
        let count = std::os::windows::fs::FileExt::seek_read(&*self.file, bytes, self.position)?;
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for PositionalArchiveReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.length.checked_add_signed(offset),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid Z2F seek offset"))?;
        Ok(self.position)
    }
}
