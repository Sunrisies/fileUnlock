use std::path::Path;
use std::process;

use crate::console::{print_green, print_red, print_yellow};
use crate::utils::paths_equivalent;
use crate::win_ffi::*;

/// 获取进程的可执行文件完整路径
pub fn get_process_exe_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);

        if ok == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}

/// 获取进程的启动命令行
pub fn get_process_cmd_line(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            return None;
        }

        // 获取 UNICODE_STRING（包含指向实际字符串的指针）
        let mut us: UNICODE_STRING_REMOTE = std::mem::zeroed();
        let mut ret_len: u32 = 0;

        let status = NtQueryInformationProcess(
            handle,
            PROCESS_COMMAND_LINE_INFORMATION,
            &mut us as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<UNICODE_STRING_REMOTE>() as u32,
            &mut ret_len,
        );

        if status != STATUS_SUCCESS || us.buffer.is_null() || us.length == 0 {
            CloseHandle(handle);
            return None;
        }

        // 读取远程进程内存中的命令行字符串
        let byte_len = us.length as usize;
        let mut buf: Vec<u16> = vec![0u16; byte_len / 2 + 1];
        let mut bytes_read: usize = 0;

        let ok = ReadProcessMemory(
            handle,
            us.buffer as *const std::ffi::c_void,
            buf.as_mut_ptr() as *mut std::ffi::c_void,
            byte_len,
            &mut bytes_read,
        );

        CloseHandle(handle);

        if ok == 0 || bytes_read == 0 {
            return None;
        }

        Some(
            String::from_utf16_lossy(&buf[..bytes_read / 2])
                .trim_end_matches('\0')
                .to_string(),
        )
    }
}

/// 打印占用进程列表
///
/// `checked_path` — 用户正在检查的文件路径，用于判断 "自身进程"
pub fn print_processes(processes: &[(u32, String)], checked_path: &str) {
    for (pid, name) in processes {
        let exe_path = get_process_exe_path(*pid);
        let is_self = exe_path
            .as_ref()
            .map(|ep| paths_equivalent(ep, checked_path))
            .unwrap_or(false);

        print!("   ");
        print_red("·");
        if is_self {
            println!(" PID {pid:<8} {name}  [自身进程]");
        } else {
            println!(" PID {pid:<8} {name}");
        }

        // 非自身进程时才显示路径（自身进程的路径就是检查的文件本身）
        if !is_self {
            if let Some(ref ep) = exe_path {
                println!("           路径: {ep}");
            }
        }

        // 成功获取到命令行时才显示
        if let Some(cmd) = get_process_cmd_line(*pid) {
            let display = if cmd.len() > 120 {
                format!("{}...", &cmd[..120])
            } else {
                cmd
            };
            println!("           命令行: {display}");
        }
    }
}

/// 结束进程 — 支持按 PID 或按名称
///
/// 自动识别:
///   `kill 61928`             → 按 PID 结束
///   `kill RealSense.Viewer`  → 按名称搜索后结束全部匹配
pub fn cmd_kill(input: &str) {
    // 尝试按 PID（纯数字）
    if let Ok(pid) = input.parse::<u32>() {
        return kill_by_pid(pid);
    }

    // 否则按名称搜索并结束
    kill_by_name(input);
}

/// 按 PID 结束单个进程
fn kill_by_pid(pid: u32) {
    let exe_path = get_process_exe_path(pid);
    if exe_path.is_none() {
        print_yellow("⚠ 进程不存在");
        println!("  PID {pid}");
        process::exit(2);
    }
    let name = exe_path
        .as_ref()
        .and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            print_red("❌ 结束失败");
            eprintln!("  无法打开进程 PID {pid}（权限不足）");
            process::exit(1);
        }

        let ret = TerminateProcess(handle, 1);
        CloseHandle(handle);

        if ret == 0 {
            print_red("❌ 结束失败");
            eprintln!("  PID {pid}  {name}");
            process::exit(1);
        }

        print_green("✅ 已结束");
        println!("  PID {pid}  {name}");
        if let Some(ref ep) = exe_path {
            println!("       {ep}");
        }
    }
}

/// 按名称搜索并结束所有匹配的进程
fn kill_by_name(name: &str) {
    let matches = find_processes(name);
    if matches.is_empty() {
        print_yellow(&format!(" 未找到匹配的进程: {name}\n"));
        return;
    }

    print_yellow(&format!(" 搜索进程: {name}\n"));
    let mut killed = 0;
    let mut failed = 0;

    for (pid, exe_name, _, _) in &matches {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, *pid);
            if handle == 0 || handle == INVALID_HANDLE_VALUE {
                print_red("  ✗");
                println!(" PID {pid:<8} {exe_name}  (权限不足)");
                failed += 1;
                continue;
            }

            let ret = TerminateProcess(handle, 1);
            CloseHandle(handle);

            if ret == 0 {
                print_red("  ✗");
                println!(" PID {pid:<8} {exe_name}");
                failed += 1;
            } else {
                print_green("  ✓");
                let show_name = exe_name.strip_suffix(".exe").unwrap_or(exe_name);
                println!(" PID {pid:<8} {show_name}");
                killed += 1;
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

/// 按名称搜索进程，返回 (PID, exe名, 路径, 是否路径匹配)
pub fn find_processes(name: &str) -> Vec<(u32, String, Option<String>, bool)> {
    let mut results = Vec::new();
    let name_lower = name.to_lowercase();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return results;
        }

        let mut pe: PROCESSENTRY32W = std::mem::zeroed();
        pe.dw_size = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut pe) != 0 {
            loop {
                let exe_name = String::from_utf16_lossy(&pe.sz_exe_file)
                    .trim_end_matches('\0')
                    .to_string();

                let name_match = exe_name.to_lowercase().contains(&name_lower);
                let pid = pe.th32_process_id;

                let (matched, is_path_match, exe_path) = if name_match {
                    (true, false, get_process_exe_path(pid))
                } else {
                    let path = get_process_exe_path(pid);
                    let matched = path
                        .as_ref()
                        .map(|p| p.to_lowercase().contains(&name_lower))
                        .unwrap_or(false);
                    (matched, matched, path)
                };

                if matched {
                    results.push((pid, exe_name, exe_path, is_path_match));
                }

                if Process32NextW(snapshot, &mut pe) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
    }

    results
}

pub fn cmd_ps(name: &str) {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            print_red("❌ 无法创建进程快照\n");
            process::exit(2);
        }

        let mut pe: PROCESSENTRY32W = std::mem::zeroed();
        pe.dw_size = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        let name_lower = name.to_lowercase();
        let mut found = false;

        if Process32FirstW(snapshot, &mut pe) != 0 {
            loop {
                let exe_name = String::from_utf16_lossy(&pe.sz_exe_file)
                    .trim_end_matches('\0')
                    .to_string();

                let name_match = exe_name.to_lowercase().contains(&name_lower);
                let pid = pe.th32_process_id;

                // 只有 exe 名不匹配时才查路径（避免不必要的系统调用）
                let (matched, is_path_match, exe_path) = if name_match {
                    (true, false, get_process_exe_path(pid))
                } else {
                    let path = get_process_exe_path(pid);
                    let matched = path
                        .as_ref()
                        .map(|p| p.to_lowercase().contains(&name_lower))
                        .unwrap_or(false);
                    (matched, matched, path)
                };

                if matched {
                    if !found {
                        print_yellow(&format!(" 搜索进程: {name}\n"));
                        found = true;
                    }
                    let ppid = pe.th32_parent_process_id;
                    let threads = pe.cnt_threads;
                    let cmd_line = get_process_cmd_line(pid);

                    let show_name = exe_name.strip_suffix(".exe").unwrap_or(&exe_name);
                    let tag = if is_path_match { " [路径匹配]" } else { "" };
                    print!("   ");
                    print_red("·");
                    println!(" PID {pid:<8} {show_name}{tag}");
                    println!("           线程: {threads:<4}  父PID: {ppid}");

                    // 始终显示完整路径
                    if let Some(ref ep) = exe_path {
                        println!("           路径: {ep}");
                    }

                    // 命令行如果获取到就显示
                    if let Some(cmd) = cmd_line {
                        let display = if cmd.len() > 150 {
                            format!("{}...", &cmd[..150])
                        } else {
                            cmd
                        };
                        println!("           命令行: {display}");
                    }
                }

                if Process32NextW(snapshot, &mut pe) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);

        if !found {
            print_yellow(&format!(" 未找到匹配的进程: {name}\n"));
        }
    }
}
