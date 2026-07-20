use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use crate::win_ffi::*;

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
pub fn check_in_use(path: &str) -> Result<bool, String> {
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

/// 通过 Restart Manager API 找出哪些进程正在占用文件。
///
/// 返回 `(PID, 进程名)` 列表，自动过滤掉自身。
pub fn find_locking_processes(path: &str) -> Result<Vec<(u32, String)>, String> {
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
