use std::path::Path;

/// 判断两个路径是否指向同一位置（大小写不敏感、相对/绝对归一化）
pub fn paths_equivalent(a: &str, b: &str) -> bool {
    let to_abs = |p: &str| -> String {
        let path = std::path::Path::new(p);
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_default()
                .join(path)
        };
        abs.to_string_lossy()
            .trim_end_matches(&['\\', '/'][..])
            .to_lowercase()
    };
    to_abs(a) == to_abs(b)
}

/// 复制文件到目标位置，然后删除源文件（用于跨卷移动 fallback）
pub fn try_copy_then_delete(src: &Path, dst: &str) -> Result<(), String> {
    std::fs::copy(src, dst).map_err(|e| format!("复制失败: {e}"))?;
    std::fs::remove_file(src).map_err(|e| format!("删除源文件失败: {e}"))?;
    Ok(())
}
