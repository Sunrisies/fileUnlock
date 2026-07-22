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

    fn find_process_by_port(&self, port: u16) -> Result<Vec<PortBinding>, String> {
        let mut results = Vec::new();

        match self.find_tcp_by_port(port) {
            Ok(mut tcp) => results.append(&mut tcp),
            Err(e) => return Err(e),
        }

        match self.find_udp_by_port(port) {
            Ok(mut udp) => results.append(&mut udp),
            Err(e) => return Err(e),
        }

        // Deduplicate by PID+protocol
        results.sort_by(|a, b| a.pid.cmp(&b.pid).then(a.protocol.cmp(&b.protocol)));
        results.dedup_by(|a, b| a.pid == b.pid && a.protocol == b.protocol);

        Ok(results)
    }

    fn find_ports_by_pid(&self, pid: u32) -> Vec<PortBinding> {
        let mut results = Vec::new();

        for af in &[AF_INET, AF_INET6] {
            // TCP
            if let Ok(bindings) = self.query_all_tcp(*af) {
                for b in bindings {
                    if b.pid == pid {
                        results.push(b);
                    }
                }
            }
            // UDP
            if let Ok(bindings) = self.query_all_udp(*af) {
                for b in bindings {
                    if b.pid == pid {
                        results.push(b);
                    }
                }
            }
        }

        // 去重
        results.sort_by(|a, b| a.port.cmp(&b.port).then(a.protocol.cmp(&b.protocol)));
        results.dedup_by(|a, b| a.port == b.port && a.protocol == b.protocol);
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

/// 获取进程的启动命令行（通过 PEB 读取，兼容性更好）
fn get_process_cmd_line(pid: u32) -> Option<String> {
    unsafe {
        // Step 1: Open with LIMITED first, then try full
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            return None;
        }

        // Step 2: Get PBI (Process Basic Information) to find PEB address
        #[repr(C)]
        struct ProcessBasicInformation {
            exit_status: isize,
            peb_base_address: *mut u8,
            affinity_mask: usize,
            base_priority: i32,
            unique_process_id: usize,
            inherited_from_unique_process_id: usize,
        }

        let mut pbi: ProcessBasicInformation = std::mem::zeroed();
        let mut ret_len: u32 = 0;

        let status = NtQueryInformationProcess(
            handle, 0,
            &mut pbi as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<ProcessBasicInformation>() as u32,
            &mut ret_len,
        );

        if status != STATUS_SUCCESS || pbi.peb_base_address.is_null() {
            CloseHandle(handle);
            return None;
        }

        #[repr(C)]
        struct PebPartial {
            reserved: [u8; 32],           // offset 0x00-0x1F (64-bit)
            process_parameters: *mut u8,  // offset 0x20
        }

        let peb_addr = pbi.peb_base_address;
        let mut peb: PebPartial = std::mem::zeroed();
        let mut bytes_read: usize = 0;

        let ok = ReadProcessMemory(
            handle, peb_addr as *const std::ffi::c_void,
            &mut peb as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<PebPartial>(), &mut bytes_read,
        );

        if ok == 0 || peb.process_parameters.is_null() {
            CloseHandle(handle);
            return None;
        }

        // CommandLine UNICODE_STRING is at offset 0x70 in RTL_USER_PROCESS_PARAMETERS (Win64)
        let cmdline_offset = 0x70usize;
        let params_addr = peb.process_parameters;

        #[repr(C)]
        struct UnicodeStringRaw {
            length: u16,
            maximum_length: u16,
            _pad: u32,
            buffer: *mut u16,
        }

        let mut us: UnicodeStringRaw = std::mem::zeroed();
        let ok = ReadProcessMemory(
            handle, params_addr.add(cmdline_offset) as *const std::ffi::c_void,
            &mut us as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<UnicodeStringRaw>(), &mut bytes_read,
        );

        if ok == 0 || us.buffer.is_null() || us.length == 0 {
            CloseHandle(handle);
            return None;
        }

        // Step 5: Read the actual command line string
        let byte_len = us.length as usize;
        let mut cmd_buf: Vec<u16> = vec![0u16; byte_len / 2 + 1];
        let ok = ReadProcessMemory(
            handle,
            us.buffer as *const std::ffi::c_void,
            cmd_buf.as_mut_ptr() as *mut std::ffi::c_void,
            byte_len,
            &mut bytes_read,
        );

        CloseHandle(handle);

        if ok == 0 || bytes_read == 0 {
            return None;
        }

        Some(
            String::from_utf16_lossy(&cmd_buf[..bytes_read / 2])
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

// ─── 端口查询 FFI (iphlpapi.dll) ────────────────────────

use super::PortBinding;

#[link(name = "iphlpapi")]
unsafe extern "system" {
    fn GetExtendedTcpTable(
        p_tcp_table: *mut std::ffi::c_void,
        pdw_size: *mut u32,
        b_order: i32,
        ul_af: u32,
        table_class: u32,
        reserved: u32,
    ) -> u32;

    fn GetExtendedUdpTable(
        p_udp_table: *mut std::ffi::c_void,
        pdw_size: *mut u32,
        b_order: i32,
        ul_af: u32,
        table_class: u32,
        reserved: u32,
    ) -> u32;
}

// TCP/UDP table class for owner PID
const TCP_TABLE_OWNER_PID_ALL: u32 = 5;
const UDP_TABLE_OWNER_PID: u32 = 1;

// Address families
const AF_INET: u32 = 2;   // IPv4
const AF_INET6: u32 = 23; // IPv6

// MIB_TCPROW_OWNER_PID: dwState, dwLocalAddr, dwLocalPort, dwRemoteAddr, dwRemotePort, dwOwningPid
#[repr(C)]
struct MibTcpRowOwnerPid {
    dw_state: u32,
    dw_local_addr: u32,
    dw_local_port: u32,    // network byte order (big-endian)
    dw_remote_addr: u32,
    dw_remote_port: u32,
    dw_owing_pid: u32,
}

// MIB_UDPROW_OWNER_PID
#[repr(C)]
struct MibUdpRowOwnerPid {
    dw_local_addr: u32,
    dw_local_port: u32,
    dw_owing_pid: u32,
}

// MIB_TCP6ROW_OWNER_PID (IPv6)
#[repr(C)]
struct MibTcp6RowOwnerPid {
    dw_local_addr: [u8; 16],
    dw_local_scope_id: u32,
    dw_local_port: u32,
    dw_remote_addr: [u8; 16],
    dw_remote_scope_id: u32,
    dw_remote_port: u32,
    dw_state: u32,        // ← state 先
    dw_owing_pid: u32,    // ← pid 后
}

// MIB_UDP6ROW_OWNER_PID (IPv6)
#[repr(C)]
struct MibUdp6RowOwnerPid {
    dw_local_addr: [u8; 16],
    dw_local_scope_id: u32,
    dw_local_port: u32,
    dw_owing_pid: u32,
}

/// Parse IPv4 address from u32 (network byte order) to string
fn ipv4_from_u32(addr: u32) -> String {
    // addr is in host byte order from the table
    let a = (addr & 0xFF) as u8;
    let b = ((addr >> 8) & 0xFF) as u8;
    let c = ((addr >> 16) & 0xFF) as u8;
    let d = ((addr >> 24) & 0xFF) as u8;
    format!("{a}.{b}.{c}.{d}")
}

/// Parse port from u32 (stored as network byte order in table)
fn port_from_u32(port: u32) -> u16 {
    ((port & 0xFFFF) as u16).to_be()
}

/// Parse IPv6 address from 16-byte array to string
fn ipv6_from_bytes(addr: &[u8; 16]) -> String {
    // Check if it's IPv4-mapped IPv6 (::ffff:x.x.x.x)
    if addr[0..10] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        && addr[10] == 0xff
        && addr[11] == 0xff
    {
        return format!("{}.{}.{}.{}", addr[12], addr[13], addr[14], addr[15]);
    }

    // Check for all-zeros (unspecified / ::)
    if addr == &[0u8; 16] {
        return "::".to_string();
    }

    // Check for loopback (::1)
    if addr[0..15] == [0u8; 15] && addr[15] == 1 {
        return "::1".to_string();
    }

    // General IPv6 formatting: split into u16 groups
    let mut groups = [0u16; 8];
    for i in 0..8 {
        groups[i] = ((addr[i * 2] as u16) << 8) | (addr[i * 2 + 1] as u16);
    }

    // Find longest run of zeros for :: compression
    let (mut best_start, mut best_len) = (8, 0);
    let (mut cur_start, mut cur_len) = (8, 0);
    for i in 0..8 {
        if groups[i] == 0 {
            if cur_len == 0 {
                cur_start = i;
            }
            cur_len += 1;
            if cur_len > best_len {
                best_start = cur_start;
                best_len = cur_len;
            }
        } else {
            cur_len = 0;
        }
    }

    if best_len < 2 {
        // No compression
        return groups
            .iter()
            .map(|g| format!("{g:x}"))
            .collect::<Vec<_>>()
            .join(":");
    }

    // Build with :: compression
    let mut parts = Vec::new();
    for i in 0..best_start {
        parts.push(format!("{:x}", groups[i]));
    }
    parts.push(String::new()); // empty string for ::
    for i in (best_start + best_len)..8 {
        parts.push(format!("{:x}", groups[i]));
    }
    parts.join(":")
}

impl WindowsPlatform {
    /// Query TCP table (IPv4 + IPv6) for matching port
    fn find_tcp_by_port(&self, port: u16) -> Result<Vec<PortBinding>, String> {
        let mut results = Vec::new();

        // IPv4
        self.query_tcp_table(AF_INET, port, &mut results)?;
        // IPv6
        self.query_tcp_table(AF_INET6, port, &mut results)?;

        Ok(results)
    }

    /// Query UDP table (IPv4 + IPv6) for matching port
    fn find_udp_by_port(&self, port: u16) -> Result<Vec<PortBinding>, String> {
        let mut results = Vec::new();

        // IPv4
        self.query_udp_table(AF_INET, port, &mut results)?;
        // IPv6
        self.query_udp_table(AF_INET6, port, &mut results)?;

        Ok(results)
    }

    fn query_tcp_table(&self, af: u32, port: u16, results: &mut Vec<PortBinding>) -> Result<(), String> {
        unsafe {
            let mut size: u32 = 0;

            let ret = GetExtendedTcpTable(
                std::ptr::null_mut(), &mut size, 1, af, TCP_TABLE_OWNER_PID_ALL, 0,
            );

            if ret != 0 && ret != 122 {
                // 122 = ERROR_INSUFFICIENT_BUFFER
                return Ok(()); // Not an error — just no entries
            }

            let mut buffer = vec![0u8; size as usize];

            let ret = GetExtendedTcpTable(
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                &mut size, 1, af, TCP_TABLE_OWNER_PID_ALL, 0,
            );

            if ret != 0 {
                return Ok(());
            }

            let num_entries = *(buffer.as_ptr() as *const u32);
            let row_base = buffer.as_ptr().add(4);

            if af == AF_INET {
                let row_ptr = row_base as *const MibTcpRowOwnerPid;
                for i in 0..num_entries as usize {
                    let row = &*row_ptr.add(i);
                    let local_port = port_from_u32(row.dw_local_port);
                    if local_port == port {
                        let addr = format!("{}:{}", ipv4_from_u32(row.dw_local_addr), local_port);
                        let pid = row.dw_owing_pid;
                        let (name, exe_path, cmd_line) = get_process_name_and_path(pid);
                        results.push(PortBinding { pid, port: local_port, protocol: "TCP".to_string(), local_addr: addr, process_name: name, exe_path, cmd_line });
                    }
                }
            } else {
                let row_ptr = row_base as *const MibTcp6RowOwnerPid;
                for i in 0..num_entries as usize {
                    let row = &*row_ptr.add(i);
                    let local_port = port_from_u32(row.dw_local_port);
                    if local_port == port {
                        let addr = format!("[{}]:{}", ipv6_from_bytes(&row.dw_local_addr), local_port);
                        let pid = row.dw_owing_pid;
                        let (name, exe_path, cmd_line) = get_process_name_and_path(pid);
                        results.push(PortBinding { pid, port: local_port, protocol: "TCP".to_string(), local_addr: addr, process_name: name, exe_path, cmd_line });
                    }
                }
            }

            Ok(())
        }
    }

    fn query_udp_table(&self, af: u32, port: u16, results: &mut Vec<PortBinding>) -> Result<(), String> {
        unsafe {
            let mut size: u32 = 0;

            let ret = GetExtendedUdpTable(
                std::ptr::null_mut(), &mut size, 1, af, UDP_TABLE_OWNER_PID, 0,
            );

            if ret != 0 && ret != 122 {
                return Ok(());
            }

            let mut buffer = vec![0u8; size as usize];

            let ret = GetExtendedUdpTable(
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                &mut size, 1, af, UDP_TABLE_OWNER_PID, 0,
            );

            if ret != 0 {
                return Ok(());
            }

            let num_entries = *(buffer.as_ptr() as *const u32);
            let row_base = buffer.as_ptr().add(4);

            if af == AF_INET {
                let row_ptr = row_base as *const MibUdpRowOwnerPid;
                for i in 0..num_entries as usize {
                    let row = &*row_ptr.add(i);
                    let local_port = port_from_u32(row.dw_local_port);
                    if local_port == port {
                        let addr = format!("{}:{}", ipv4_from_u32(row.dw_local_addr), local_port);
                        let pid = row.dw_owing_pid;
                        let (name, exe_path, cmd_line) = get_process_name_and_path(pid);
                        results.push(PortBinding { pid, port: local_port, protocol: "UDP".to_string(), local_addr: addr, process_name: name, exe_path, cmd_line });
                    }
                }
            } else {
                let row_ptr = row_base as *const MibUdp6RowOwnerPid;
                for i in 0..num_entries as usize {
                    let row = &*row_ptr.add(i);
                    let local_port = port_from_u32(row.dw_local_port);
                    if local_port == port {
                        let addr = format!("[{}]:{}", ipv6_from_bytes(&row.dw_local_addr), local_port);
                        let pid = row.dw_owing_pid;
                        let (name, exe_path, cmd_line) = get_process_name_and_path(pid);
                        results.push(PortBinding { pid, port: local_port, protocol: "UDP".to_string(), local_addr: addr, process_name: name, exe_path, cmd_line });
                    }
                }
            }

            Ok(())
        }
    }

    /// 返回指定地址族的所有 TCP 绑定（不过滤端口，不获取进程信息）
    fn query_all_tcp(&self, af: u32) -> Result<Vec<PortBinding>, String> {
        unsafe {
            let mut size: u32 = 0;
            let ret = GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 1, af, TCP_TABLE_OWNER_PID_ALL, 0);
            if ret != 0 && ret != 122 { return Ok(vec![]); }
            let mut buffer = vec![0u8; size as usize];
            let ret = GetExtendedTcpTable(buffer.as_mut_ptr() as *mut std::ffi::c_void, &mut size, 1, af, TCP_TABLE_OWNER_PID_ALL, 0);
            if ret != 0 { return Ok(vec![]); }

            let num = *(buffer.as_ptr() as *const u32);
            let base = buffer.as_ptr().add(4);
            let mut results = Vec::new();

            if af == AF_INET {
                for i in 0..num as usize {
                    let row = &*(base as *const MibTcpRowOwnerPid).add(i);
                    let port = port_from_u32(row.dw_local_port);
                    results.push(PortBinding { pid: row.dw_owing_pid, port, protocol: "TCP".into(), local_addr: format!("{}:{}", ipv4_from_u32(row.dw_local_addr), port), process_name: String::new(), exe_path: None, cmd_line: None });
                }
            } else {
                for i in 0..num as usize {
                    let row = &*(base as *const MibTcp6RowOwnerPid).add(i);
                    let port = port_from_u32(row.dw_local_port);
                    results.push(PortBinding { pid: row.dw_owing_pid, port, protocol: "TCP".into(), local_addr: format!("[{}]:{}", ipv6_from_bytes(&row.dw_local_addr), port), process_name: String::new(), exe_path: None, cmd_line: None });
                }
            }
            Ok(results)
        }
    }

    /// 返回指定地址族的所有 UDP 绑定（不过滤端口，不获取进程信息）
    fn query_all_udp(&self, af: u32) -> Result<Vec<PortBinding>, String> {
        unsafe {
            let mut size: u32 = 0;
            let ret = GetExtendedUdpTable(std::ptr::null_mut(), &mut size, 1, af, UDP_TABLE_OWNER_PID, 0);
            if ret != 0 && ret != 122 { return Ok(vec![]); }
            let mut buffer = vec![0u8; size as usize];
            let ret = GetExtendedUdpTable(buffer.as_mut_ptr() as *mut std::ffi::c_void, &mut size, 1, af, UDP_TABLE_OWNER_PID, 0);
            if ret != 0 { return Ok(vec![]); }

            let num = *(buffer.as_ptr() as *const u32);
            let base = buffer.as_ptr().add(4);
            let mut results = Vec::new();

            if af == AF_INET {
                for i in 0..num as usize {
                    let row = &*(base as *const MibUdpRowOwnerPid).add(i);
                    let port = port_from_u32(row.dw_local_port);
                    results.push(PortBinding { pid: row.dw_owing_pid, port, protocol: "UDP".into(), local_addr: format!("{}:{}", ipv4_from_u32(row.dw_local_addr), port), process_name: String::new(), exe_path: None, cmd_line: None });
                }
            } else {
                for i in 0..num as usize {
                    let row = &*(base as *const MibUdp6RowOwnerPid).add(i);
                    let port = port_from_u32(row.dw_local_port);
                    results.push(PortBinding { pid: row.dw_owing_pid, port, protocol: "UDP".into(), local_addr: format!("[{}]:{}", ipv6_from_bytes(&row.dw_local_addr), port), process_name: String::new(), exe_path: None, cmd_line: None });
                }
            }
            Ok(results)
        }
    }
}

/// Get process name and exe path by PID (helper for port lookup)
fn get_process_name_and_path(pid: u32) -> (String, Option<String>, Option<String>) {
    let exe_path = get_process_exe_path(pid);
    let name = exe_path
        .as_ref()
        .and_then(|p| std::path::Path::new(p).file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("[PID {pid}]"));
    let cmd_line = get_process_cmd_line(pid);
    (name, exe_path, cmd_line)
}
