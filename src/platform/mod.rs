/// 平台抽象 trait —— 所有操作系统相关的操作都通过此接口
pub trait Platform {
    /// 检查文件/文件夹是否被其他进程占用
    /// - `Ok(true)`  = 被占用
    /// - `Ok(false)` = 未被占用
    /// - `Err`       = 路径不存在或其他错误
    fn check_file_in_use(&self, path: &str) -> Result<bool, String>;

    /// 查找哪些进程正在占用指定文件，返回进程信息列表
    fn find_locking_processes(&self, path: &str) -> Result<Vec<ProcessInfo>, String>;

    /// 按名称模糊搜索正在运行的进程
    fn find_processes(&self, name: &str) -> Vec<ProcessInfo>;

    /// 获取指定 PID 进程的详细信息
    fn get_process_info(&self, pid: u32) -> Option<ProcessInfo>;

    /// 终止指定 PID 的进程
    fn kill_process(&self, pid: u32) -> Result<(), String>;

    /// 在 PATH 中查找可执行文件，返回所有匹配的完整路径
    fn find_in_path(&self, name: &str) -> Vec<String>;

    /// 查找占用指定端口的进程，返回绑定信息列表
    fn find_process_by_port(&self, port: u16) -> Result<Vec<PortBinding>, String>;
}

/// 平台无关的进程信息
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub exe_path: Option<String>,
    pub cmd_line: Option<String>,
    pub parent_pid: Option<u32>,
    pub thread_count: Option<u32>,
}

/// 端口绑定信息
#[derive(Debug, Clone)]
pub struct PortBinding {
    pub pid: u32,
    pub port: u16,
    pub protocol: String,      // TCP / UDP
    pub local_addr: String,    // e.g. "0.0.0.0:3000"
    pub process_name: String,
    pub exe_path: Option<String>,
    pub cmd_line: Option<String>,
}

// ─── 平台实现选择 ──────────────────────────────────────

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::WindowsPlatform as CurrentPlatform;

// TODO: 后续添加 Linux/macOS 实现
// #[cfg(target_os = "linux")]
// mod linux;
// #[cfg(target_os = "linux")]
// pub use linux::LinuxPlatform as CurrentPlatform;
