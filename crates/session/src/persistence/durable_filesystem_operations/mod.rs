use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

pub(super) fn read_file_with_maximum_byte_count(
    file_path: &Path,
    maximum_byte_count: usize,
) -> io::Result<Vec<u8>> {
    let mut file_bytes = Vec::new();
    read_file_into_reused_buffer_with_maximum_byte_count(
        file_path,
        maximum_byte_count,
        &mut file_bytes,
    )?;
    Ok(file_bytes)
}

pub(super) fn read_file_into_reused_buffer_with_maximum_byte_count(
    file_path: &Path,
    maximum_byte_count: usize,
    file_bytes: &mut Vec<u8>,
) -> io::Result<()> {
    let file = File::open(file_path)?;
    let declared_file_byte_count = usize::try_from(file.metadata()?.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "file is too large"))?;
    if declared_file_byte_count > maximum_byte_count {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file is too large",
        ));
    }
    file_bytes.clear();
    file_bytes.reserve(declared_file_byte_count);
    file.take((maximum_byte_count as u64).saturating_add(1))
        .read_to_end(file_bytes)?;
    if file_bytes.len() != declared_file_byte_count {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "file length changed",
        ));
    }
    Ok(())
}

pub(super) fn atomically_write_and_sync_file(
    destination_file_path: &Path,
    file_bytes: &[u8],
) -> io::Result<()> {
    let parent_directory_path = destination_file_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(parent_directory_path)?;
    let destination_file_name = destination_file_path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no filename"))?;
    let temporary_file_path =
        parent_directory_path.join(format!(".{}.tmp", destination_file_name.to_string_lossy()));
    let write_result = (|| {
        let mut temporary_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_file_path)?;
        temporary_file.write_all(file_bytes)?;
        temporary_file.sync_all()?;
        drop(temporary_file);
        replace_file_and_sync_metadata(&temporary_file_path, destination_file_path)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_file_path);
    }
    write_result
}

pub(super) fn durably_delete_file_if_present(file_path: &Path) -> io::Result<()> {
    match fs::remove_file(file_path) {
        Ok(()) => {
            // Windows has no unprivileged directory flush equivalent. Its delete
            // completes normally, but cannot promise power-loss durability here.
            #[cfg(not(windows))]
            if let Some(parent_directory_path) = file_path.parent() {
                File::open(parent_directory_path)?.sync_all()?;
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
fn replace_file_and_sync_metadata(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)?;
    if let Some(parent) = destination.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn replace_file_and_sync_metadata(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let encode_path = |path: &Path| -> io::Result<Vec<u16>> {
        let mut encoded: Vec<u16> = path.as_os_str().encode_wide().collect();
        if encoded.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path contains a null character",
            ));
        }
        encoded.push(0);
        Ok(encoded)
    };
    // Canonical parents also give Win32 its extended-length path prefix.
    let source = encode_path(&fs::canonicalize(source)?)?;
    let destination_parent = destination
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    let destination_name = destination
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no filename"))?;
    let destination = encode_path(&fs::canonicalize(destination_parent)?.join(destination_name))?;
    // Both buffers are null-terminated and remain alive throughout the call.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
