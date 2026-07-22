use std::path::Path;
use std::process;

mod console;
use console::{print_green, print_red, print_yellow};

mod cli;
use cli::print_usage;

mod utils;
use utils::try_copy_then_delete;

#[cfg(target_os = "windows")]
mod win_ffi;

mod platform;
use platform::{Platform, CurrentPlatform};

mod proc;
use proc::{cmd_kill, cmd_ps};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // 处理 -h / --help
    if args.len() == 2 && (args[1] == "-h" || args[1] == "--help") {
        print_usage(&args[0]);
        process::exit(0);
    }

    if args.len() < 3 {
        print_usage(&args[0]);
        process::exit(1);
    }

    let platform = CurrentPlatform;
    let cmd = args[1].as_str();
    let path = &args[2];

    // 检查全局 --json flag
    let json = args.iter().any(|a| a == "--json");

    match cmd {
        "check" | "检查" => cmd_check(&platform, path),
        "delete" | "删除" => cmd_delete(&platform, path),
        "ps" | "进程" => cmd_ps(&platform, path, json),
        "kill" | "结束" => cmd_kill(&platform, path),
        "port" | "端口" => cmd_port(&platform, path),
        "where" | "查找" | "which" => cmd_where(&platform, path),
        "rename" | "重命名" | "move" | "移动" => {
            if args.len() < 4 {
                eprintln!("用法: {} rename <源路径> <目标路径>", args[0]);
                eprintln!("       {} move    <源路径> <目标路径>", args[0]);
                process::exit(1);
            }
            let dst = &args[3];
            cmd_rename(&platform, path, dst);
        }
        _ => {
            print_usage(&args[0]);
            process::exit(1);
        }
    }
}

// ─── 子命令实现 ────────────────────────────────────────

/// 打印占用进程列表（平台无关版本）
fn print_process_list(processes: &[platform::ProcessInfo], checked_path: &str) {
    for info in processes {
        let is_self = info
            .exe_path
            .as_ref()
            .map(|ep| utils::paths_equivalent(ep, checked_path))
            .unwrap_or(false);

        print!("   ");
        print_red("·");
        if is_self {
            println!(" PID {:<8} {}  [自身进程]", info.pid, info.name);
        } else {
            println!(" PID {:<8} {}", info.pid, info.name);
        }

        if !is_self {
            if let Some(ref ep) = info.exe_path {
                println!("           路径: {ep}");
            }
        }

        if let Some(ref cmd) = info.cmd_line {
            let display = if cmd.len() > 120 {
                format!("{}...", &cmd[..120])
            } else {
                cmd.clone()
            };
            println!("           命令行: {display}");
        }
    }
}

fn cmd_check(platform: &impl Platform, path: &str) {
    match platform.check_file_in_use(path) {
        Ok(true) => {
            print_red("❌ 占用中");
            println!("  {path}");
            match platform.find_locking_processes(path) {
                Ok(processes) if !processes.is_empty() => {
                    print_process_list(&processes, path);
                }
                Ok(_) => {
                    print_yellow("     未能获取到占用进程详情\n");
                }
                Err(e) => {
                    print_yellow(&format!("     {e}\n"));
                }
            }
            process::exit(1);
        }
        Ok(false) => {
            print_green("✅ 未占用");
            println!("  {path}");
        }
        Err(e) => {
            print_yellow("⚠ 未知");
            println!("  {e}");
            process::exit(2);
        }
    }
}

fn cmd_delete(platform: &impl Platform, path: &str) {
    let p = Path::new(path);
    if !p.exists() {
        print_yellow("⚠ 不存在");
        println!("  路径不存在: {path}");
        process::exit(2);
    }

    match platform.check_file_in_use(path) {
        Ok(true) => {
            print_red("❌ 删除失败");
            println!("  文件正在被其他程序使用，无法删除: {path}");
            if let Ok(processes) = platform.find_locking_processes(path) {
                if !processes.is_empty() {
                    print_process_list(&processes, path);
                }
            }
            process::exit(1);
        }
        Ok(false) => {
            let result = if p.is_dir() {
                std::fs::remove_dir_all(p)
            } else {
                std::fs::remove_file(p)
            };
            match result {
                Ok(()) => {
                    print_green("✅ 已删除");
                    println!("  {path}");
                }
                Err(e) => {
                    print_red("❌ 删除失败");
                    eprintln!("  {e}");
                    process::exit(2);
                }
            }
        }
        Err(e) => {
            print_red("❌ 删除失败");
            eprintln!("  {e}");
            process::exit(2);
        }
    }
}

fn cmd_rename(platform: &impl Platform, src: &str, dst: &str) {
    let src_path = Path::new(src);
    if !src_path.exists() {
        print_yellow("⚠ 不存在");
        println!("  源路径不存在: {src}");
        process::exit(2);
    }

    match platform.check_file_in_use(src) {
        Ok(true) => {
            print_red("❌ 移动/重命名失败");
            println!("  文件正在被其他程序使用，无法操作: {src}");
            if let Ok(processes) = platform.find_locking_processes(src) {
                if !processes.is_empty() {
                    print_process_list(&processes, src);
                }
            }
            process::exit(1);
        }
        Ok(false) => {
            match std::fs::rename(src_path, dst) {
                Ok(()) => {
                    print_green("✅ 已移动/重命名");
                    println!("  {src} → {dst}");
                }
                Err(e) => {
                    if src_path.is_file() {
                        match try_copy_then_delete(src_path, dst) {
                            Ok(()) => {
                                print_green("✅ 已移动/重命名（跨卷复制）");
                                println!("  {src} → {dst}");
                            }
                            Err(e2) => {
                                print_red("❌ 移动失败");
                                eprintln!("  重命名错误: {e}");
                                eprintln!("  复制模式错误: {e2}");
                                process::exit(2);
                            }
                        }
                    } else {
                        print_red("❌ 移动失败");
                        eprintln!("  {e}");
                        eprintln!("  提示: 目录跨卷移动请手动复制后删除");
                        process::exit(2);
                    }
                }
            }
        }
        Err(e) => {
            print_red("❌ 移动/重命名失败");
            eprintln!("  {e}");
            process::exit(2);
        }
    }
}

fn cmd_where(platform: &impl Platform, name: &str) {
    let results = platform.find_in_path(name);

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

fn cmd_port(platform: &impl Platform, port_str: &str) {
    let port: u16 = match port_str.parse() {
        Ok(p) => p,
        Err(_) => {
            print_red("❌ 无效端口号");
            eprintln!("  {port_str}");
            process::exit(2);
        }
    };

    match platform.find_process_by_port(port) {
        Ok(bindings) if bindings.is_empty() => {
            print_yellow(&format!(" 未发现占用端口 {port} 的进程\n"));
        }
        Ok(bindings) => {
            print_yellow(&format!(" 端口 {port} 占用情况:\n"));
            for b in &bindings {
                print!("   ");
                print_green("·");
                println!(
                    " PID {:<8} {}  [{}:{}] ",
                    b.pid, b.process_name, b.protocol, b.port
                );
                println!("           地址: {}", b.local_addr);
                if let Some(ref ep) = b.exe_path {
                    println!("           路径: {ep}");
                }
                if let Some(ref cmd) = b.cmd_line {
                    println!("           命令: {cmd}");
                }
            }
        }
        Err(e) => {
            print_red("❌ 查询失败");
            eprintln!("  {e}");
            process::exit(2);
        }
    }
}
