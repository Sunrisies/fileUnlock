# WhoUse

**跨平台进程/端口/文件占用查询工具** — 告诉你究竟是哪个进程在占用文件、监听端口、或运行程序。

支持 **Windows** 和 **Linux**，中文/英文双语命令。

## 效果

```text
$ WhoUse port 8080
🔍 端口 8080 占用情况:
   · PID 43256    node.exe  [TCP:8080]
           地址: [::1]:8080
           路径: C:\nvm4w\nodejs\node.exe
           命令: node "D:\project\puko-admin\node_modules\vite\bin\vite.js"

$ WhoUse ps java
🔍 搜索进程: java
   · PID 5406     java
           路径: /work/inst/jdk1.8.0_144/bin/java
           命令行: java -Xms512m -Xmx2048m -jar 3sai-admin-1.0.0.jar
           端口: 8080, 8848

$ WhoUse check D:\locked.txt
❌ 占用中  D:\locked.txt
   · PID 21428    Windows PowerShell
           路径: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
```

## 安装

### 从源码编译

```bash
git clone https://github.com/Sunrisies/whouse.git
cd whouse

# Windows
cargo build --release

# Linux
cargo build --release
# 或静态编译（不依赖 glibc）
cargo build --release --target x86_64-unknown-linux-musl
```

### Windows 一键安装

```bash
install.bat
```

安装脚本会自动复制到 `%USERPROFILE%\.whouse\` 并加入 PATH。装好后可用三个等价名字：

```bash
inuse ps notepad         # 短名，推荐
who check Cargo.toml     # 短名
WhoUse where node    # 原名
```

## 用法

| 命令 | 中文别名 | 说明 |
|------|----------|------|
| `check <路径>` | `检查` | 检查文件/文件夹是否被占用 |
| `delete <路径>` | `删除` | 安全删除（先检查，被占用则拒绝） |
| `rename <旧> <新>` | `重命名` | 安全重命名/移动（先检查源文件） |
| `move <旧> <新>` | `移动` | 同上 |
| `ps <进程名>` | `进程` | 搜索进程（显示路径、命令行、**监听端口**） |
| `kill <PID/名>` | `结束` | 结束进程（按 PID 或名称） |
| `port <端口号>` | `端口` | **查找占用指定端口的进程** |
| `where <程序名>` | `查找` | 在 PATH 中搜索程序位置 |

### 端口查询

```bash
# 查看谁在用 3000 端口
WhoUse port 3000
WhoUse 端口 3000

# 输出示例
🔍 端口 3000 占用情况:
   · PID 12345    node.exe  [TCP:3000]
           地址: 0.0.0.0:3000
           路径: C:\nvm4w\nodejs\node.exe
           命令: node server.js
```

支持 IPv4 和 IPv6，同时查询 TCP 和 UDP。

### 进程搜索（含端口）

```bash
# 搜索进程并显示监听端口
WhoUse ps java
WhoUse ps nginx

# 输出示例
🔍 搜索进程: java
   · PID 5406     java
           路径: /work/inst/jdk1.8.0_144/bin/java
           命令行: java -Xms512m -Xmx2048m -jar 3sai-admin-1.0.0.jar
           端口: 8080, 8848
```

### 文件锁检测

```bash
# 检查文件是否被占用
WhoUse check D:\资料\报告.docx
WhoUse 检查 Cargo.toml

# 安全删除（被占用则拒绝并显示占用进程）
WhoUse delete D:\temp\locked.txt
WhoUse 删除 D:\temp\locked.txt

# 安全重命名/移动
WhoUse rename old.txt new.txt
WhoUse 移动 source.exe D:\backup\
```

### 结束进程

```bash
# 按 PID 结束
WhoUse kill 61928

# 按名称搜索并结束全部匹配
WhoUse kill notepad
WhoUse 结束 notepad
```

### 查找程序

```bash
WhoUse where node
WhoUse 查找 python
```

### 退出码

| 退出码 | 含义 |
|--------|------|
| `0` | 操作成功，或文件未被占用 |
| `1` | 文件正被占用，操作被拒绝 |
| `2` | 路径不存在或其他错误 |

## 架构

```
src/
├── main.rs              CLI 入口 + 子命令路由
├── platform/
│   ├── mod.rs           Platform trait 定义 + 条件编译
│   ├── windows.rs       Windows 实现（FFI 调用 Win32 API）
│   └── linux.rs         Linux 实现（/proc 文件系统）
├── proc.rs              进程管理命令（ps / kill）
├── console.rs           跨平台彩色输出（Win32 API / ANSI 转义码）
├── cli.rs               帮助文本
├── utils.rs             工具函数
└── win_ffi.rs           Windows FFI 声明（仅 Windows 编译）
```

### 跨平台设计

通过 `Platform` trait 抽象所有平台相关操作：

```rust
pub trait Platform {
    fn check_file_in_use(&self, path: &str) -> Result<bool, String>;
    fn find_locking_processes(&self, path: &str) -> Result<Vec<ProcessInfo>, String>;
    fn find_processes(&self, name: &str) -> Vec<ProcessInfo>;
    fn get_process_info(&self, pid: u32) -> Option<ProcessInfo>;
    fn kill_process(&self, pid: u32) -> Result<(), String>;
    fn find_in_path(&self, name: &str) -> Vec<String>;
    fn find_process_by_port(&self, port: u16) -> Result<Vec<PortBinding>, String>;
    fn find_ports_by_pid(&self, pid: u32) -> Vec<PortBinding>;
}
```

| 功能 | Windows | Linux |
|------|---------|-------|
| 文件锁检测 | `CreateFileW` 独占模式 | `lsof` |
| 锁定进程查询 | Restart Manager API | `lsof` |
| 进程搜索 | Toolhelp32 快照 | `/proc` 遍历 |
| 进程信息 | `QueryFullProcessImageNameW` + PEB | `/proc/[pid]/exe` + `cmdline` |
| 命令行读取 | PEB → ProcessParameters → CommandLine | `/proc/[pid]/cmdline` |
| 端口查询 | `GetExtendedTcpTable` / `GetExtendedUdpTable` (iphlpapi.dll) | `/proc/net/tcp{,6}` + inode 反查 |
| 结束进程 | `TerminateProcess` | `kill(SIGTERM)` |
| PATH 搜索 | `PATH` + `PATHEXT` | `PATH` |
| 彩色输出 | Win32 `SetConsoleTextAttribute` | ANSI 转义码 |

### 端口查询原理

**Windows:**
```
GetExtendedTcpTable(AF_INET/AF_INET6)
  → 遍历所有 TCP 连接，按端口过滤
  → 获取 PID
GetExtendedUdpTable(AF_INET/AF_INET6)
  → 遍历所有 UDP 绑定，按端口过滤
```

**Linux:**
```
/proc/net/tcp{,6} → 解析十六进制地址:端口，获取 socket inode
        ↓
/proc/[pid]/fd/ → 遍历所有进程的 fd，匹配 socket:[inode] → PID
        ↓
/proc/[pid]/cmdline → 读取完整命令行
```

## 技术细节

### 零外部依赖（Windows）

Windows 版本通过 **raw FFI** 直接调用系统 API，不依赖任何第三方 crate：

| DLL | API |
|-----|-----|
| `kernel32.dll` | `CreateFileW`, `CloseHandle`, `OpenProcess`, `QueryFullProcessImageNameW`, `ReadProcessMemory` |
| `rstrtmgr.dll` | `RmStartSession`, `RmRegisterResources`, `RmGetList`, `RmEndSession` |
| `ntdll.dll` | `NtQueryInformationProcess` |
| `iphlpapi.dll` | `GetExtendedTcpTable`, `GetExtendedUdpTable` |

### Linux 依赖

Linux 版本仅依赖 `libc` crate（用于 `kill` 系统调用），其余通过 `/proc` 文件系统实现。

## 系统要求

- **Windows**: Windows 7+
- **Linux**: 任意主流发行版（Ubuntu、CentOS、Debian 等）
- **Rust**: 1.85+（edition 2024）

## 协议

MIT
