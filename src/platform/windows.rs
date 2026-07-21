use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use crate::win_ffi::*;
use super::{Platform, ProcessInfo};

pub struct WindowsPlatform;

impl Platform for WindowsPlatform {
    fn check_file_in_use(&self, path: &str) -> Result<bool, String> {
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
                FILE_SHARE_NONE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                flags,
                std::ptr::null_mut(),
            );

            if handle != INVALID_HANDLE_VALUE {
                CloseHandle(handle);
                return Ok(false);
            }

            let err = get_last_error();

            match err {
                ERROR_SHARING_VIOLATION => Ok(true),
                ERROR_ACCESS_DENIED => {
                    if is_dir {
                        Err(format!("访问被拒绝，可能没有足够权限"))
                    } else {
                        Ok(true)
                    }
                }
                e => Err(format!("无法访问路径 (系统错误 {e})")),
            }
        }
    }

    fn find_locking_processes(&self, path: &str) -> Result<Vec<ProcessInfo>, String> {
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

            let processes: Vec<ProcessInfo> = buffer
                .iter()
                .take(proc_info_count as usize)
                .filter(|p| p.process.dw_process_id != own_pid)
                .map(|p| {
                    let name = String::from_utf16_lossy(&p.str_app_name)
                        .trim_end_matches('\0')
                        .to_string();
                    let pid = p.process.dw_process_id;
                    ProcessInfo {
                        pid,
                        name,
                        exe_path: get_process_exe_path(pid),
                        cmd_line: get_process_cmd_line(pid),
                        parent_pid: None,
                        thread_count: None,
                    }
                })
                .collect();

            Ok(processes)
        }
    }

    fn find_processes(&self, name: &str) -> Vec<ProcessInfo> {
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

                    let (matched, exe_path) = if name_match {
                        (true, get_process_exe_path(pid))
                    } else {
                        let path = get_process_exe_path(pid);
                        let matched = path
                            .as_ref()
                            .map(|p| p.to_lowercase().contains(&name_lower))
                            .unwrap_or(false);
                        (matched, path)
                    };

                    if matched {
                        results.push(ProcessInfo {
                            pid,
                            name: exe_name,
                            exe_path,
                            cmd_line: get_process_cmd_line(pid),
                            parent_pid: Some(pe.th32_parent_process_id),
                            thread_count: Some(pe.cnt_threads),
                        });
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

    fn get_process_info(&self, pid: u32) -> Option<ProcessInfo> {
        let exe_path = get_process_exe_path(pid)?;
        let name = Path::new(&exe_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        Some(ProcessInfo {
            pid,
            name,
            exe_path: Some(exe_path),
            cmd_line: get_process_cmd_line(pid),
            parent_pid: None,
            thread_count: None,
        })
    }

    fn kill_process(&self, pid: u32) -> Result<(), String> {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if handle == 0 || handle == INVALID_HANDLE_VALUE {
                return Err(format!("无法打开进程 PID {pid}（权限不足）"));
            }

            let ret = TerminateProcess(handle, 1);
            CloseHandle(handle);

            if ret == 0 {
                return Err(format!("终止进程 PID {pid} 失败"));
            }

            Ok(())
        }
    }

    fn find_in_path(&self, name: &str) -> Vec<String> {
        let p = Path::new(name);
        if p.is_absolute() && p.exists() {
            return vec![name.to_string()];
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

        results.sort();
        results.dedup();
        results
    }
}

/// 将路径转为 UTF-16 宽字符串（以 null 结尾）
fn to_wide(path: &str) -> Vec<u16> {
    OsStr::new(path).encode_wide().chain(std::iter::once(0)).collect()
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

fn get_last_error() -> u32 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
    }
    unsafe { GetLastError() }
}
