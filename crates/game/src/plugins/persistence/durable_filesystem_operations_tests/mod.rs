use std::{fs, path::PathBuf};

use super::durable_filesystem_operations::{
    atomically_write_and_sync_file, durably_delete_file_if_present,
    read_file_with_maximum_byte_count,
};

#[test]
fn atomic_replace_never_exposes_partial_bytes_and_delete_is_idempotent() {
    let temporary_directory = create_unique_temporary_directory_for_persistence_test("atomic");
    let save_slot_file_path = temporary_directory.join("nested/save.ozs");
    atomically_write_and_sync_file(&save_slot_file_path, b"first complete snapshot").unwrap();
    assert_eq!(
        read_file_with_maximum_byte_count(&save_slot_file_path, 1024).unwrap(),
        b"first complete snapshot"
    );
    atomically_write_and_sync_file(&save_slot_file_path, b"second").unwrap();
    assert_eq!(
        read_file_with_maximum_byte_count(&save_slot_file_path, 1024).unwrap(),
        b"second"
    );
    assert!(!temporary_directory.join("nested/.save.ozs.tmp").exists());
    durably_delete_file_if_present(&save_slot_file_path).unwrap();
    durably_delete_file_if_present(&save_slot_file_path).unwrap();
    assert!(!save_slot_file_path.exists());
    fs::remove_dir_all(temporary_directory).unwrap();
}

#[test]
fn failed_atomic_replace_preserves_the_previous_target() {
    let temporary_directory =
        create_unique_temporary_directory_for_persistence_test("failed-atomic");
    let occupied_directory_path = temporary_directory.join("occupied");
    fs::create_dir_all(&occupied_directory_path).unwrap();
    fs::write(occupied_directory_path.join("sentinel"), b"unchanged").unwrap();
    assert!(atomically_write_and_sync_file(&occupied_directory_path, b"replacement").is_err());
    assert_eq!(
        fs::read(occupied_directory_path.join("sentinel")).unwrap(),
        b"unchanged"
    );
    assert!(!temporary_directory.join(".occupied.tmp").exists());
    fs::remove_dir_all(temporary_directory).unwrap();
}

fn create_unique_temporary_directory_for_persistence_test(test_directory_label: &str) -> PathBuf {
    let unique_timestamp_nanoseconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openzt2-persistence-{test_directory_label}-{unique_timestamp_nanoseconds}"
    ))
}
