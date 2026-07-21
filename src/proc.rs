use std::process;

use crate::console::{print_green, print_red, print_yellow};
use crate::platform::Platform;

/// 结束进程 — 支持按 PID 或按名称
pub fn cmd_kill(platform: &impl Platform, input: &str) {
    // 尝试按 PID（纯数字）
    if let Ok(pid) = input.parse::<u32>() {
        return kill_by_pid(platform, pid);
    }

    // 否则按名称搜索并结束
    kill_by_name(platform, input);
}

/// 按 PID 结束单个进程
fn kill_by_pid(platform: &impl Platform, pid: u32) {
    let info = platform.get_process_info(pid);
    if info.is_none() {
        print_yellow("⚠ 进程不存在");
        println!("  PID {pid}");
        process::exit(2);
    }
    let info = info.unwrap();
    let name = &info.name;

    match platform.kill_process(pid) {
        Ok(()) => {
            print_green("✅ 已结束");
            println!("  PID {pid}  {name}");
            if let Some(ref ep) = info.exe_path {
                println!("       {ep}");
            }
        }
        Err(e) => {
            print_red("❌ 结束失败");
            eprintln!("  {e}");
            process::exit(1);
        }
    }
}

/// 按名称搜索并结束所有匹配的进程
fn kill_by_name(platform: &impl Platform, name: &str) {
    let matches = platform.find_processes(name);
    if matches.is_empty() {
        print_yellow(&format!(" 未找到匹配的进程: {name}\n"));
        return;
    }

    print_yellow(&format!(" 搜索进程: {name}\n"));
    let mut killed = 0;
    let mut failed = 0;

    for info in &matches {
        match platform.kill_process(info.pid) {
            Ok(()) => {
                print_green("  ✓");
                let show_name = info.name.strip_suffix(".exe").unwrap_or(&info.name);
                println!(" PID {:<8} {show_name}", info.pid);
                killed += 1;
            }
            Err(_) => {
                print_red("  ✗");
                println!(" PID {:<8} {}  (权限不足)", info.pid, info.name);
                failed += 1;
            }
        }
    }

    println!();
    if failed == 0 {
        print_green(&format!("✅ 共结束 {killed} 个进程\n"));
    } else {
        print_yellow(&format!(" 结束 {killed} 个，{failed} 个失败\n"));
    }
}

pub fn cmd_ps(platform: &impl Platform, name: &str) {
    let matches = platform.find_processes(name);

    if matches.is_empty() {
        print_yellow(&format!(" 未找到匹配的进程: {name}\n"));
        return;
    }

    print_yellow(&format!(" 搜索进程: {name}\n"));

    for info in &matches {
        let show_name = info.name.strip_suffix(".exe").unwrap_or(&info.name);
        print!("   ");
        print_red("·");
        println!(" PID {:<8} {show_name}", info.pid);

        if let Some(ppid) = info.parent_pid {
            let threads = info.thread_count.unwrap_or(0);
            println!("           线程: {threads:<4}  父PID: {ppid}");
        }

        if let Some(ref ep) = info.exe_path {
            println!("           路径: {ep}");
        }

        if let Some(ref cmd) = info.cmd_line {
            let display = if cmd.len() > 150 {
                format!("{}...", &cmd[..150])
            } else {
                cmd.clone()
            };
            println!("           命令行: {display}");
        }
    }
}
