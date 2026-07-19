use std::path::Path;

use crate::console::{print_green, print_yellow};

/// 在 PATH 中查找可执行文件的位置（类似 Windows `where` 命令）
pub fn cmd_where(name: &str) {
    // 如果已经是完整路径且存在，直接返回
    let p = Path::new(name);
    if p.is_absolute() && p.exists() {
        print_yellow(&format!(" 查找: {name}\n"));
        print!("   ");
        print_green("✓");
        println!(" {name}");
        return;
    }

    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let dirs: Vec<_> = std::env::split_paths(&path_var).collect();

    let pathext_var = std::env::var_os("PATHEXT")
        .unwrap_or_else(|| std::ffi::OsString::from(".exe;.com;.bat;.cmd"));
    let pathext: Vec<String> = pathext_var
        .to_string_lossy()
        .split(';')
        .map(|s| s.to_lowercase())
        .collect();

    let has_ext = pathext.iter().any(|ext| name.to_lowercase().ends_with(ext));
    let name_lower = name.to_lowercase();

    let mut results: Vec<String> = Vec::new();

    for dir in &dirs {
        if !dir.is_dir() {
            continue;
        }

        if has_ext {
            let full = dir.join(name);
            if full.exists() {
                results.push(full.to_string_lossy().to_string());
            }
        } else {
            for ext in &pathext {
                let candidate = format!("{}{}", name_lower, ext);
                let full = dir.join(&candidate);
                if full.exists() {
                    results.push(full.to_string_lossy().to_string());
                    break;
                }
            }
        }
    }

    // 去重
    results.sort();
    results.dedup();

    if results.is_empty() {
        print_yellow(&format!(" 未找到匹配: {name}\n"));
        return;
    }

    print_yellow(&format!(" 查找: {name}\n"));
    for path in &results {
        print!("   ");
        print_green("✓");
        println!(" {path}");
    }
    if results.len() > 1 {
        println!("  共找到 {} 个位置", results.len());
    }
}
