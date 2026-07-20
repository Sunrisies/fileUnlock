use std::path::Path;
use std::process;

mod console;
use console::{print_green, print_red, print_yellow};

mod cli;
use cli::print_usage;

mod where_cmd;
use where_cmd::cmd_where;

mod utils;
use utils::try_copy_then_delete;

mod win_ffi;

mod proc;
use proc::{cmd_kill, cmd_ps, print_processes};

mod file_lock;
use file_lock::{check_in_use, find_locking_processes};

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

    let cmd = args[1].as_str();
    let path = &args[2];

    match cmd {
        "check" | "检查" => cmd_check(path),
        "delete" | "删除" => cmd_delete(path),
        "ps" | "进程" => cmd_ps(path),
        "kill" | "结束" => cmd_kill(path),
        "where" | "查找" | "which" => cmd_where(path),
        "rename" | "重命名" | "move" | "移动" => {
            if args.len() < 4 {
                eprintln!("用法: {} rename <源路径> <目标路径>", args[0]);
                eprintln!("       {} move    <源路径> <目标路径>", args[0]);
                process::exit(1);
            }
            let dst = &args[3];
            cmd_rename(path, dst);
        }
        _ => {
            print_usage(&args[0]);
            process::exit(1);
        }
    }
}

// ─── 子命令实现 ────────────────────────────────────────

fn cmd_check(path: &str) {
    match check_in_use(path) {
        Ok(true) => {
            print_red("❌ 占用中");
            println!("  {path}");
            // 尝试获取占用进程详情
            match find_locking_processes(path) {
                Ok(processes) if !processes.is_empty() => {
                    print_processes(&processes, path);
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

fn cmd_delete(path: &str) {
    let p = Path::new(path);
    if !p.exists() {
        print_yellow("⚠ 不存在");
        println!("  路径不存在: {path}");
        process::exit(2);
    }

    match check_in_use(path) {
        Ok(true) => {
            print_red("❌ 删除失败");
            println!("  文件正在被其他程序使用，无法删除: {path}");
            if let Ok(processes) = find_locking_processes(path) {
                if !processes.is_empty() {
                    print_processes(&processes, path);
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

fn cmd_rename(src: &str, dst: &str) {
    let src_path = Path::new(src);
    if !src_path.exists() {
        print_yellow("⚠ 不存在");
        println!("  源路径不存在: {src}");
        process::exit(2);
    }

    match check_in_use(src) {
        Ok(true) => {
            print_red("❌ 移动/重命名失败");
            println!("  文件正在被其他程序使用，无法操作: {src}");
            if let Ok(processes) = find_locking_processes(src) {
                if !processes.is_empty() {
                    print_processes(&processes, src);
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
                    // 重命名失败（可能是跨卷），尝试复制+删除
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
