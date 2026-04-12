use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process,
};

use uuid::Uuid;

#[cfg(windows)]
use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr};

#[cfg(windows)]
const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
#[cfg(windows)]
const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
#[cfg(windows)]
const REPLACEFILE_WRITE_THROUGH: u32 = 0x0000_0001;

fn format_atomic_write_error(path: &Path, action: &str, error: std::io::Error) -> String {
    match error.raw_os_error() {
        Some(32) | Some(33) => format!(
            "无法{action}：目标文件正被其他程序占用，当前不能写回。\n请先关闭正在使用该文件的程序（如 Word/WPS、资源管理器预览窗格、同步盘或杀毒软件）后重试。\n文件：{}\n系统错误：{error}",
            path.display()
        ),
        _ => error.to_string(),
    }
}

pub(crate) fn write_bytes_atomically(path: &Path, payload: &[u8]) -> Result<(), String> {
    let parent = parent_dir(path);
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    let temp_path = build_temp_path(path)?;
    write_temp_file(&temp_path, payload)?;
    if let Err(error) = replace_temp_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    sync_parent_dir(parent)?;
    Ok(())
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn build_temp_path(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "目标文件路径缺少文件名，无法安全写回。".to_string())?;
    Ok(parent_dir(path).join(format!(
        ".{file_name}.tmp-{}-{}",
        process::id(),
        Uuid::new_v4()
    )))
}

fn write_temp_file(path: &Path, payload: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format_atomic_write_error(path, "写入临时文件", error))?;
    file.write_all(payload)
        .map_err(|error| format_atomic_write_error(path, "写入临时文件", error))?;
    file.sync_all()
        .map_err(|error| format_atomic_write_error(path, "刷新临时文件", error))
}

#[cfg(windows)]
fn replace_temp_file(temp_path: &Path, target_path: &Path) -> Result<(), String> {
    if target_path.exists() {
        replace_existing_file_windows(temp_path, target_path)
    } else {
        move_file_windows(temp_path, target_path)
    }
}

#[cfg(not(windows))]
fn replace_temp_file(temp_path: &Path, target_path: &Path) -> Result<(), String> {
    fs::rename(temp_path, target_path)
        .map_err(|error| format_atomic_write_error(target_path, "替换目标文件", error))
}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> Result<(), String> {
    fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn encode_wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn replace_existing_file_windows(temp_path: &Path, target_path: &Path) -> Result<(), String> {
    let replaced = encode_wide(target_path);
    let replacement = encode_wide(temp_path);
    let result = unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if result == 0 {
        return Err(format_atomic_write_error(
            target_path,
            "替换目标文件",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn move_file_windows(temp_path: &Path, target_path: &Path) -> Result<(), String> {
    let existing = encode_wide(temp_path);
    let target = encode_wide(target_path);
    let result = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format_atomic_write_error(
            target_path,
            "移动临时文件到目标位置",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
extern "system" {
    fn ReplaceFileW(
        lp_replaced_file_name: *const u16,
        lp_replacement_file_name: *const u16,
        lp_backup_file_name: *const u16,
        dw_replace_flags: u32,
        lp_exclude: *mut c_void,
        lp_reserved: *mut c_void,
    ) -> i32;

    fn MoveFileExW(
        lp_existing_file_name: *const u16,
        lp_new_file_name: *const u16,
        dw_flags: u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
    };

    use super::format_atomic_write_error;
    use super::write_bytes_atomically;
    use uuid::Uuid;

    fn unique_test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!("lessai-{name}-{}", Uuid::new_v4()))
    }

    fn cleanup_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn atomic_write_creates_parent_dirs_and_new_file() {
        let root = unique_test_dir("atomic-create");
        let target = root.join("nested").join("document.txt");
        let payload = b"first version";

        write_bytes_atomically(&target, payload).expect("atomic write");

        let stored = fs::read(&target).expect("read file");
        assert_eq!(stored, payload);

        cleanup_dir(&root);
    }

    #[test]
    fn atomic_write_replaces_existing_file_contents_without_leaking_temp_files() {
        let root = unique_test_dir("atomic-replace");
        fs::create_dir_all(&root).expect("create root");
        let target = root.join("draft.docx");
        fs::write(&target, b"old").expect("seed old file");

        write_bytes_atomically(&target, b"new").expect("atomic replace");

        let stored = fs::read(&target).expect("read file");
        assert_eq!(stored, b"new");

        let leaked = fs::read_dir(&root)
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(".draft.docx.tmp-"))
            .collect::<Vec<_>>();
        assert!(leaked.is_empty(), "temp files leaked: {leaked:?}");

        cleanup_dir(&root);
    }

    #[test]
    fn atomic_write_formats_windows_sharing_violation_clearly() {
        let message = format_atomic_write_error(
            Path::new("E:/Docs/report.docx"),
            "替换目标文件",
            std::io::Error::from_raw_os_error(32),
        );

        assert!(message.contains("被其他程序占用"));
        assert!(message.contains("Word") || message.contains("WPS"));
        assert!(message.contains("report.docx"));
    }
}
