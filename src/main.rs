use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
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
use win_ffi::*;

mod proc;
use proc::{cmd_kill, cmd_ps, print_processes};

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

// ─── 核心：检测文件/文件夹是否被占用 ────────────────────

/// 将路径转为 UTF-16 宽字符串（以 null 结尾）
fn to_wide(path: &str) -> Vec<u16> {
    OsStr::new(path).encode_wide().chain(std::iter::once(0)).collect()
}

/// 检查路径是否正被其他进程占用。
///
/// 返回:
/// - `Ok(true)`  — 文件/文件夹正在被使用
/// - `Ok(false)` — 文件/文件夹未被占用
/// - `Err(msg)`  — 路径不存在或其他错误
fn check_in_use(path: &str) -> Result<bool, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(format!("路径不存在: {path}"));
    }

    let is_dir = p.is_dir();
    let wide = to_wide(path);

    unsafe {
        let desired_access = if is_dir {
            FILE_LIST_DIRECTORY
        } else {
            GENERIC_READ | GENERIC_WRITE
        };

        let flags = if is_dir {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            0
        };

        let handle = CreateFileW(
            wide.as_ptr(),
            desired_access,
            FILE_SHARE_NONE, // 独占模式 — 不共享
            std::ptr::null_mut(),
            OPEN_EXISTING,
            flags,
            std::ptr::null_mut(),
        );

        if handle != INVALID_HANDLE_VALUE {
            // 成功打开 -> 未被占用
            CloseHandle(handle);
            return Ok(false);
        }

        let err = get_last_error();

        match err {
            ERROR_SHARING_VIOLATION => {
                // 共享冲突 -> 文件被其他进程打开
                Ok(true)
            }
            ERROR_ACCESS_DENIED => {
                if is_dir {
                    Err(format!("访问被拒绝，可能没有足够权限"))
                } else {
                    // 文件可能被独占打开
                    Ok(true)
                }
            }
            e => Err(format!("无法访问路径 (系统错误 {e})")),
        }
    }
}

/// 包装 `kernel32!GetLastError`
fn get_last_error() -> u32 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
    }
    unsafe { GetLastError() }
}

// ─── 查找占用进程（Restart Manager API）──────────────────

/// 通过 Restart Manager API 找出哪些进程正在占用文件。
///
/// 返回 `(PID, 进程名)` 列表，自动过滤掉自身。
fn find_locking_processes(path: &str) -> Result<Vec<(u32, String)>, String> {
    let wide_path = to_wide(path);

    unsafe {
        let mut session_handle: u32 = 0;
        let mut session_key = [0u16; CCH_RM_SESSION_KEY];

        let ret = RmStartSession(
            &mut session_handle,
            0,
            session_key.as_mut_ptr(),
        );
        if ret != ERROR_SUCCESS {
            if ret == ERROR_ACCESS_DENIED_RM {
                return Err("权限不足，请以管理员身份运行".into());
            }
            return Err(format!("RmStartSession 失败 (error {ret})"));
        }

        // 注册文件到会话
        let path_ptrs: [*const u16; 1] = [wide_path.as_ptr()];

        let ret = RmRegisterResources(
            session_handle,
            1,
            path_ptrs.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        );
        if ret != ERROR_SUCCESS {
            RmEndSession(session_handle);
            return Err(format!("RmRegisterResources 失败 (error {ret})"));
        }

        // 第一次调用：获取需要的缓冲区大小
        let mut proc_info_needed: u32 = 0;
        let mut proc_info_count: u32 = 0;
        let mut reboot_reasons: u32 = 0;

        let _ = RmGetList(
            session_handle,
            &mut proc_info_needed,
            &mut proc_info_count,
            std::ptr::null_mut(),
            &mut reboot_reasons,
        );

        if proc_info_needed == 0 {
            RmEndSession(session_handle);
            return Ok(vec![]);
        }

        // 第二次调用：实际获取进程列表
        proc_info_count = proc_info_needed;
        let mut buffer: Vec<RM_PROCESS_INFO> = vec![
            std::mem::zeroed::<RM_PROCESS_INFO>();
            proc_info_needed as usize
        ];

        let ret = RmGetList(
            session_handle,
            &mut proc_info_needed,
            &mut proc_info_count,
            buffer.as_mut_ptr(),
            &mut reboot_reasons,
        );

        RmEndSession(session_handle);

        if ret != ERROR_SUCCESS {
            return Err(format!("RmGetList 失败 (error {ret})"));
        }

        let own_pid = std::process::id();

        let processes: Vec<(u32, String)> = buffer
            .iter()
            .take(proc_info_count as usize)
            .filter(|p| p.process.dw_process_id != own_pid)
            .map(|p| {
                let name = String::from_utf16_lossy(&p.str_app_name)
                    .trim_end_matches('\0')
                    .to_string();
                (p.process.dw_process_id, name)
            })
            .collect();

        Ok(processes)
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
