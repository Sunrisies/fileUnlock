use std::fs;
use std::path::Path;

use super::{Platform, ProcessInfo, PortBinding};

pub struct LinuxPlatform;

impl Platform for LinuxPlatform {
    fn check_file_in_use(&self, path: &str) -> Result<bool, String> {
        // 使用 lsof 检查文件是否被占用
        let output = std::process::Command::new("lsof")
            .arg(path)
            .output()
            .map_err(|e| format!("执行 lsof 失败: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().count() > 1) // 第一行是表头
    }

    fn find_locking_processes(&self, path: &str) -> Result<Vec<ProcessInfo>, String> {
        let output = std::process::Command::new("lsof")
            .arg(path)
            .output()
            .map_err(|e| format!("执行 lsof 失败: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();

        for line in stdout.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 2 {
                if let Ok(pid) = fields[1].parse::<u32>() {
                    results.push(ProcessInfo {
                        pid,
                        name: fields[0].to_string(),
                        exe_path: read_exe_path(pid),
                        cmd_line: read_cmdline(pid),
                        parent_pid: None,
                        thread_count: None,
                    });
                }
            }
        }

        Ok(results)
    }

    fn find_processes(&self, name: &str) -> Vec<ProcessInfo> {
        let name_lower = name.to_lowercase();
        let mut results = Vec::new();

        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let pid_str = file_name.to_string_lossy();

                if let Ok(pid) = pid_str.parse::<u32>() {
                    let comm = read_comm(pid);
                    let exe_path = read_exe_path(pid);

                    let matched = comm
                        .as_ref()
                        .map(|c| c.to_lowercase().contains(&name_lower))
                        .unwrap_or(false)
                        || exe_path
                            .as_ref()
                            .map(|p| p.to_lowercase().contains(&name_lower))
                            .unwrap_or(false);

                    if matched {
                        results.push(ProcessInfo {
                            pid,
                            name: comm.unwrap_or_else(|| format!("[{pid}]")),
                            exe_path,
                            cmd_line: read_cmdline(pid),
                            parent_pid: read_ppid(pid),
                            thread_count: None,
                        });
                    }
                }
            }
        }

        results
    }

    fn get_process_info(&self, pid: u32) -> Option<ProcessInfo> {
        let exe_path = read_exe_path(pid)?;
        let name = Path::new(&exe_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| read_comm(pid).unwrap_or_else(|| format!("[{pid}]")));

        Some(ProcessInfo {
            pid,
            name,
            exe_path: Some(exe_path),
            cmd_line: read_cmdline(pid),
            parent_pid: read_ppid(pid),
            thread_count: None,
        })
    }

    fn kill_process(&self, pid: u32) -> Result<(), String> {
        unsafe {
            if libc::kill(pid as i32, libc::SIGTERM) == 0 {
                Ok(())
            } else {
                Err(format!("终止进程 PID {pid} 失败"))
            }
        }
    }

    fn find_in_path(&self, name: &str) -> Vec<String> {
        let p = Path::new(name);
        if p.is_absolute() && p.exists() {
            return vec![name.to_string()];
        }

        let path_var = std::env::var_os("PATH").unwrap_or_default();
        let mut results = Vec::new();

        for dir in std::env::split_paths(&path_var) {
            let full = dir.join(name);
            if full.exists() {
                results.push(full.to_string_lossy().to_string());
            }
        }

        results.sort();
        results.dedup();
        results
    }

    fn find_process_by_port(&self, port: u16) -> Result<Vec<PortBinding>, String> {
        let port_hex = format!("{:04X}", port);
        let mut results = Vec::new();

        // 1. 从 /proc/net/* 收集匹配端口的 (协议, 地址, inode)
        let mut entries: Vec<(String, String, String)> = Vec::new(); // (proto, addr, inode)

        for (path, proto) in &[
            ("/proc/net/tcp", "TCP"),
            ("/proc/net/tcp6", "TCP"),
            ("/proc/net/udp", "UDP"),
            ("/proc/net/udp6", "UDP"),
        ] {
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for line in content.lines().skip(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 10 {
                    continue;
                }

                let local = fields[1];
                if let Some(colon_pos) = local.rfind(':') {
                    if &local[colon_pos + 1..] == port_hex {
                        let addr_hex = &local[..colon_pos];
                        let addr = format!("{}:{}", format_addr(addr_hex), port);
                        let inode = fields[9].to_string();
                        entries.push((proto.to_string(), addr, inode));
                    }
                }
            }
        }

        // 2. 对每个 inode，遍历 /proc/*/fd/ 反查 PID
        for (proto, addr, inode) in &entries {
            let inode_tag = format!("socket:[{inode}]");
            let mut found_pid = 0u32;

            if let Ok(proc_entries) = fs::read_dir("/proc") {
                for proc_entry in proc_entries.flatten() {
                    let name = proc_entry.file_name();
                    let pid_str = name.to_string_lossy();
                    let pid: u32 = match pid_str.parse() {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    let fd_dir = format!("/proc/{pid}/fd");
                    let fds = match fs::read_dir(&fd_dir) {
                        Ok(f) => f,
                        Err(_) => continue,
                    };

                    for fd_entry in fds.flatten() {
                        if let Ok(link) = fs::read_link(fd_entry.path()) {
                            if link.to_string_lossy() == inode_tag {
                                found_pid = pid;
                                break;
                            }
                        }
                    }

                    if found_pid != 0 {
                        break;
                    }
                }
            }

            if found_pid != 0 {
                let pid = found_pid;
                results.push(PortBinding {
                    pid,
                    port,
                    protocol: proto.clone(),
                    local_addr: addr.clone(),
                    process_name: read_comm(pid)
                        .or_else(|| {
                            read_exe_path(pid).and_then(|p| {
                                Path::new(&p)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                            })
                        })
                        .unwrap_or_else(|| format!("[PID {pid}]")),
                    exe_path: read_exe_path(pid),
                    cmd_line: read_cmdline(pid),
                });
            }
        }

        // 去重（同一 PID+协议可能出现在多个表中）
        results.sort_by(|a, b| a.pid.cmp(&b.pid).then(a.protocol.cmp(&b.protocol)));
        results.dedup_by(|a, b| a.pid == b.pid && a.protocol == b.protocol && a.local_addr == b.local_addr);

        Ok(results)
    }
}

// ─── /proc/net 解析 ────────────────────────────────────

/// 根据十六进制地址字符串解析为可读 IP
fn format_addr(hex_addr: &str) -> String {
    if hex_addr.len() == 8 {
        // IPv4: "0100007F" → "127.0.0.1"
        if let Ok(num) = u32::from_str_radix(hex_addr, 16) {
            let a = (num & 0xFF) as u8;
            let b = ((num >> 8) & 0xFF) as u8;
            let c = ((num >> 16) & 0xFF) as u8;
            let d = ((num >> 24) & 0xFF) as u8;
            return format!("{a}.{b}.{c}.{d}");
        }
    } else if hex_addr.len() == 32 {
        // IPv6: "00000000000000000000000001000000" → "::1"
        let mut bytes = [0u8; 16];
        for i in 0..16 {
            if let Ok(b) = u8::from_str_radix(&hex_addr[i * 2..i * 2 + 2], 16) {
                bytes[i] = b;
            }
        }
        return ipv6_from_bytes(&bytes);
    }

    format!("unknown:{hex_addr}")
}

/// IPv6 地址格式化
fn ipv6_from_bytes(addr: &[u8; 16]) -> String {
    // Check for ::1 (loopback)
    if addr[0..15] == [0u8; 15] && addr[15] == 1 {
        return "::1".to_string();
    }

    // Check for :: (unspecified)
    if addr == &[0u8; 16] {
        return "::".to_string();
    }

    // Check for IPv4-mapped (::ffff:x.x.x.x)
    if addr[0..10] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        && addr[10] == 0xff
        && addr[11] == 0xff
    {
        return format!("{}.{}.{}.{}", addr[12], addr[13], addr[14], addr[15]);
    }

    // General: u16 groups with :: compression
    let mut groups = [0u16; 8];
    for i in 0..8 {
        groups[i] = ((addr[i * 2] as u16) << 8) | (addr[i * 2 + 1] as u16);
    }

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
        return groups
            .iter()
            .map(|g| format!("{g:x}"))
            .collect::<Vec<_>>()
            .join(":");
    }

    let mut parts = Vec::new();
    for i in 0..best_start {
        parts.push(format!("{:x}", groups[i]));
    }
    parts.push(String::new());
    for i in (best_start + best_len)..8 {
        parts.push(format!("{:x}", groups[i]));
    }
    parts.join(":")
}

// ─── /proc/[pid]/ 辅助函数 ─────────────────────────────

/// 读取 /proc/[pid]/comm（进程名）
fn read_comm(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
}

/// 读取 /proc/[pid]/exe 符号链接（可执行文件路径）
fn read_exe_path(pid: u32) -> Option<String> {
    fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

/// 读取 /proc/[pid]/cmdline（命令行参数，\0 分隔）
fn read_cmdline(pid: u32) -> Option<String> {
    let bytes = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if bytes.is_empty() {
        return None;
    }
    // cmdline 用 \0 分隔参数，替换为空格
    let args: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .collect();
    Some(args.join(" "))
}

/// 读取 /proc/[pid]/status 获取 PPid
fn read_ppid(pid: u32) -> Option<u32> {
    let content = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in content.lines() {
        if let Some(ppid_str) = line.strip_prefix("PPid:") {
            return ppid_str.trim().parse::<u32>().ok();
        }
    }
    None
}
