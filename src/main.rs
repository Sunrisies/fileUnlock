use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process;

// ─── Restart Manager API 常量 ───────────────────────────

const CCH_RM_SESSION_KEY: usize = 64;
const ERROR_SUCCESS: i32 = 0;

const ERROR_ACCESS_DENIED_RM: i32 = 5;

// ─── Windows API FFI ────────────────────────────────────

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut std::ffi::c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut std::ffi::c_void,
    ) -> isize;

    fn CloseHandle(hObject: isize) -> i32;

    fn GetStdHandle(nStdHandle: u32) -> isize;

    fn SetConsoleTextAttribute(hConsoleOutput: isize, wAttributes: u16) -> i32;

    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;

    fn QueryFullProcessImageNameW(
        hProcess: isize,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> i32;

    fn ReadProcessMemory(
        hProcess: isize,
        lpBaseAddress: *const std::ffi::c_void,
        lpBuffer: *mut std::ffi::c_void,
        nSize: usize,
        lpNumberOfBytesRead: *mut usize,
    ) -> i32;

    fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> isize;

    fn Process32FirstW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;

    fn Process32NextW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
}

// ─── ntdll API FFI ─────────────────────────────────────

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        ProcessHandle: isize,
        ProcessInformationClass: u32,
        ProcessInformation: *mut std::ffi::c_void,
        ProcessInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> i32;
}

// ─── Restart Manager API FFI ────────────────────────────

#[link(name = "rstrtmgr")]
unsafe extern "system" {
    fn RmStartSession(
        pSessionHandle: *mut u32,
        dwSessionFlags: u32,
        strSessionKey: *mut u16,
    ) -> i32;

    fn RmRegisterResources(
        dwSessionHandle: u32,
        nFiles: u32,
        rgsFilenames: *const *const u16,
        nApplications: u32,
        rgApplications: *const std::ffi::c_void,
        nServices: u32,
        rgsServiceNames: *const *const u16,
    ) -> i32;

    fn RmGetList(
        dwSessionHandle: u32,
        pnProcInfoNeeded: *mut u32,
        pnProcInfo: *mut u32,
        rgAffectedApps: *mut RM_PROCESS_INFO,
        lpdwRebootReasons: *mut u32,
    ) -> i32;

    fn RmEndSession(dwSessionHandle: u32) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PROCESSENTRY32W {
    dw_size: u32,
    cnt_usage: u32,
    th32_process_id: u32,
    th32_default_heap_id: u64,
    th32_module_id: u32,
    cnt_threads: u32,
    th32_parent_process_id: u32,
    pc_pri_class_base: i32,
    dw_flags: u32,
    sz_exe_file: [u16; 260],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UNICODE_STRING_REMOTE {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FILETIME {
    dw_low_date_time: u32,
    dw_high_date_time: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RM_UNIQUE_PROCESS {
    dw_process_id: u32,
    process_start_time: FILETIME,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RM_PROCESS_INFO {
    process: RM_UNIQUE_PROCESS,
    str_app_name: [u16; 256],
    str_service_short_name: [u16; 64],
    application_type: u32,
    app_status: u32,
    ts_session_id: u32,
    b_restartable: i32,
}

const INVALID_HANDLE_VALUE: isize = -1;
const STD_OUTPUT_HANDLE: u32 = 0xfffffff5;

// File access / share modes
const GENERIC_READ: u32 = 0x80000000;
const GENERIC_WRITE: u32 = 0x40000000;
const FILE_LIST_DIRECTORY: u32 = 0x0001;
const FILE_SHARE_NONE: u32 = 0x00000000;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;

// Windows error codes
const ERROR_SHARING_VIOLATION: u32 = 32;
const ERROR_ACCESS_DENIED: u32 = 5;

// Process access rights
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
const PROCESS_VM_READ: u32 = 0x0010;

// NtQueryInformationProcess info classes
const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;

// NTSTATUS
const STATUS_SUCCESS: i32 = 0;

// Toolhelp32
const TH32CS_SNAPPROCESS: u32 = 0x00000002;

// Console colors
const FOREGROUND_RED: u16 = 4;
const FOREGROUND_GREEN: u16 = 2;
const FOREGROUND_BLUE: u16 = 1;
const FOREGROUND_INTENSITY: u16 = 8;

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

fn print_usage(prog: &str) {
    let prog = Path::new(prog)
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed(prog));

    eprintln!("FileUnlock v1.0 — 检测并安全操作被占用的文件/文件夹 (Windows)");
    eprintln!();
    eprintln!("用法:");
    eprintln!("  {prog} check   <路径>             检查文件是否被占用");
    eprintln!("  {prog} delete  <路径>             安全删除（先检查）");
    eprintln!("  {prog} rename  <源路径> <目标>     安全重命名/移动（先检查）");
    eprintln!("  {prog} move    <源路径> <目标>     同上");
    eprintln!("  {prog} ps      <进程名>           搜索正在运行的进程");
    eprintln!();
    eprintln!("中文别名（等价）:");
    eprintln!("  check  = 检查");
    eprintln!("  delete = 删除");
    eprintln!("  rename = 重命名");
    eprintln!("  move   = 移动");
    eprintln!("  ps     = 进程");
    eprintln!();
    eprintln!("例子:");
    eprintln!("  {prog} 检查 Cargo.toml");
    eprintln!("  {prog} 删除 D:\\锁定文件.txt");
    eprintln!("  {prog} 重命名 old.txt new.txt");
    eprintln!("  {prog} move   源文件.exe D:\\备份\\");
    eprintln!();
    eprintln!("参数:");
    eprintln!("  -h, --help    显示此帮助信息");
    eprintln!();
    eprintln!("退出码:");
    eprintln!("  0  操作成功，或文件未被占用");
    eprintln!("  1  文件正被占用，操作被拒绝");
    eprintln!("  2  路径不存在或其他错误");
}

// ─── 控制台彩色输出 ─────────────────────────────────────

fn set_console_color(red: bool, green: bool, blue: bool, intensity: bool) {
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle == INVALID_HANDLE_VALUE || handle == 0 {
            return;
        }
        let mut attr = FOREGROUND_RED | FOREGROUND_GREEN | FOREGROUND_BLUE; // default white
        if red {
            attr |= FOREGROUND_RED;
        } else {
            attr &= !FOREGROUND_RED;
        }
        if green {
            attr |= FOREGROUND_GREEN;
        } else {
            attr &= !FOREGROUND_GREEN;
        }
        if blue {
            attr |= FOREGROUND_BLUE;
        } else {
            attr &= !FOREGROUND_BLUE;
        }
        if intensity {
            attr |= FOREGROUND_INTENSITY;
        }
        SetConsoleTextAttribute(handle, attr);
    }
}

fn reset_color() {
    set_console_color(true, true, true, false);
}

fn print_green(text: &str) {
    set_console_color(false, true, false, true);
    print!("{}", text);
    reset_color();
}

fn print_red(text: &str) {
    set_console_color(true, false, false, true);
    print!("{}", text);
    reset_color();
}

fn print_yellow(text: &str) {
    set_console_color(true, true, false, true);
    print!("{}", text);
    reset_color();
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

/// 获取进程的可执行文件完整路径
fn get_process_exe_path(pid: u32) -> Option<String> {
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
fn get_process_cmd_line(pid: u32) -> Option<String> {
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

/// 判断两个路径是否指向同一位置（大小写不敏感、相对/绝对归一化）
fn paths_equivalent(a: &str, b: &str) -> bool {
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

/// 打印占用进程列表
///
/// `checked_path` — 用户正在检查的文件路径，用于判断 "自身进程"
fn print_processes(processes: &[(u32, String)], checked_path: &str) {
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

// ─── 进程搜索 ──────────────────────────────────────────

/// 按名称搜索正在运行的进程（模糊匹配）
fn cmd_ps(name: &str) {
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

                if exe_name.to_lowercase().contains(&name_lower) {
                    if !found {
                        print_yellow(&format!(" 搜索进程: {name}\n"));
                        found = true;
                    }
                    let pid = pe.th32_process_id;
                    let exe_path = get_process_exe_path(pid);
                    print!("   ");
                    print_red("·");
                    // 去掉 .exe 后缀显示更干净
                    let show_name = exe_name.strip_suffix(".exe").unwrap_or(&exe_name);
                    println!(" PID {pid:<8} {show_name}");
                    if let Some(ref ep) = exe_path {
                        if !ep.ends_with(&exe_name) {
                            println!("           路径: {ep}");
                        }
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

fn try_copy_then_delete(src: &Path, dst: &str) -> Result<(), String> {
    std::fs::copy(src, dst).map_err(|e| format!("复制失败: {e}"))?;
    std::fs::remove_file(src).map_err(|e| format!("删除源文件失败: {e}"))?;
    Ok(())
}
