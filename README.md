# FileUnlock

**检测并安全操作被占用的文件/文件夹** — 当 Windows 提示"文件正在被另一程序使用"时，告诉你究竟是哪个进程在占用。

## 效果

```text
$ FileUnlock check D:\Downloads\RealSense.Viewer.exe
❌ 占用中  D:\Downloads\RealSense.Viewer.exe
   · PID 61928    RealSense Viewer  [自身进程]

$ FileUnlock delete D:\locked.txt
❌ 删除失败  文件正在被其他程序使用，无法删除: D:\locked.txt
   · PID 21428    Windows PowerShell
           路径: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
```

## 安装

```bash
git clone https://github.com/Sunrisies/fileUnlock.git
cd fileUnlock
cargo build --release
```

### 一键安装（推荐）

```bash
install.bat
```

安装脚本会自动将 `FileUnlock.exe` 复制到 `%USERPROFILE%\.fileunlock\` 并加入 PATH，
之后在任何目录下都可直接使用。

### 手动安装

编译产物在 `target\release\FileUnlock.exe`，将其所在目录加入 PATH 即可。

## 用法

| 命令 | 中文别名 | 说明 |
|------|----------|------|
| `check <路径>` | `检查 <路径>` | 检查文件/文件夹是否被占用 |
| `delete <路径>` | `删除 <路径>` | 安全删除（先检查，被占用则拒绝） |
| `rename <旧> <新>` | `重命名 <旧> <新>` | 安全重命名/移动（先检查源文件） |
| `move <旧> <新>` | `移动 <旧> <新>` | 同上 |
| `ps <进程名>` | `进程 <进程名>` | 搜索正在运行的进程 |
| `kill <PID/名>` | `结束 <PID/名>` | 结束进程（按 PID 或名称） |
| `where <程序名>` | `查找 <程序名>` | 在 PATH 中搜索程序位置 |

### 示例

```bash
# 检查
FileUnlock check D:\资料\报告.docx
FileUnlock 检查 Cargo.toml

# 安全删除
FileUnlock delete D:\Downloads\旧驱动.exe
FileUnlock 删除 D:\temp\临时文件.txt

# 安全重命名/移动
FileUnlock rename old.txt new.txt
FileUnlock 移动 source.exe D:\backup\

# 进程搜索与结束
FileUnlock ps notepad
FileUnlock kill 61928
FileUnlock 结束 notepad

# 查找程序位置
FileUnlock where node
FileUnlock 查找 python
```

### 退出码

| 退出码 | 含义 |
|--------|------|
| `0` | 操作成功，或文件未被占用 |
| `1` | 文件正被占用，操作被拒绝 |
| `2` | 路径不存在或其他错误 |

## 获取的信息

当文件被占用时，会显示以下内容：

| 信息 | 来源 | 限制 |
|------|------|------|
| **PID** | Restart Manager API | 一般都能获取 |
| **进程名** | Restart Manager API | 一般都能获取 |
| **完整路径** | `QueryFullProcessImageNameW` | 同用户进程通常可读 |
| **启动命令行** | `NtQueryInformationProcess` | 部分高权限进程可能拒绝 |

拿到 PID 后可以用系统工具处理：

```bash
taskkill /F /PID 61928       # 强制结束进程
tasklist /FI "PID eq 61928"  # 查看进程详情
```

## 技术原理

```
┌─ CreateFileW 独占模式 ─────────────────┐
│  以 FILE_SHARE_NONE 尝试打开目标路径     │
│  → 成功: 未被占用，立即关闭句柄          │
│  → ERROR_SHARING_VIOLATION: 被占用      │
└────────────────────────────────────────┘
                      ↓ 占用时继续查询
┌─ Restart Manager API ──────────────────┐
│  RmStartSession                        │
│  → RmRegisterResources(文件路径)        │
│  → RmGetList → PID + 进程名            │
│  → RmEndSession                        │
└────────────────────────────────────────┘
                      ↓ 根据 PID 补充详情
┌─ 进程信息查询 ─────────────────────────┐
│  OpenProcess + QueryFullProcessImageNameW → 路径 │
│  NtQueryInformationProcess → 命令行     │
└────────────────────────────────────────┘
```

### 零外部依赖

整个工具通过 **raw FFI** 直接调用 Windows API，不依赖任何第三方 crate：

| DLL | API |
|-----|-----|
| `kernel32.dll` | `CreateFileW`, `CloseHandle`, `OpenProcess`, `QueryFullProcessImageNameW`, `ReadProcessMemory`, `GetStdHandle`, `SetConsoleTextAttribute` |
| `rstrtmgr.dll` | `RmStartSession`, `RmRegisterResources`, `RmGetList`, `RmEndSession` |
| `ntdll.dll` | `NtQueryInformationProcess` |

## 系统要求

- Windows 7+
- Rust 1.85+（edition 2024）

## 协议

MIT
