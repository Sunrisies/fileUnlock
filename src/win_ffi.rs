// ─── Restart Manager API 常量 ───────────────────────────

pub const CCH_RM_SESSION_KEY: usize = 64;
pub const ERROR_SUCCESS: i32 = 0;
pub const ERROR_ACCESS_DENIED_RM: i32 = 5;

// ─── Windows API FFI ────────────────────────────────────

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut std::ffi::c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut std::ffi::c_void,
    ) -> isize;

    pub fn CloseHandle(hObject: isize) -> i32;

    pub fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;

    pub fn QueryFullProcessImageNameW(
        hProcess: isize,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> i32;

    pub fn ReadProcessMemory(
        hProcess: isize,
        lpBaseAddress: *const std::ffi::c_void,
        lpBuffer: *mut std::ffi::c_void,
        nSize: usize,
        lpNumberOfBytesRead: *mut usize,
    ) -> i32;

    pub fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> isize;

    pub fn Process32FirstW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;

    pub fn Process32NextW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;

    pub fn TerminateProcess(hProcess: isize, uExitCode: u32) -> i32;
}

// ─── ntdll API FFI ─────────────────────────────────────

#[link(name = "ntdll")]
unsafe extern "system" {
    pub fn NtQueryInformationProcess(
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
    pub fn RmStartSession(
        pSessionHandle: *mut u32,
        dwSessionFlags: u32,
        strSessionKey: *mut u16,
    ) -> i32;

    pub fn RmRegisterResources(
        dwSessionHandle: u32,
        nFiles: u32,
        rgsFilenames: *const *const u16,
        nApplications: u32,
        rgApplications: *const std::ffi::c_void,
        nServices: u32,
        rgsServiceNames: *const *const u16,
    ) -> i32;

    pub fn RmGetList(
        dwSessionHandle: u32,
        pnProcInfoNeeded: *mut u32,
        pnProcInfo: *mut u32,
        rgAffectedApps: *mut RM_PROCESS_INFO,
        lpdwRebootReasons: *mut u32,
    ) -> i32;

    pub fn RmEndSession(dwSessionHandle: u32) -> i32;
}

// ─── FFI 数据结构 ──────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PROCESSENTRY32W {
    pub dw_size: u32,
    pub cnt_usage: u32,
    pub th32_process_id: u32,
    pub th32_default_heap_id: u64,
    pub th32_module_id: u32,
    pub cnt_threads: u32,
    pub th32_parent_process_id: u32,
    pub pc_pri_class_base: i32,
    pub dw_flags: u32,
    pub sz_exe_file: [u16; 260],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UNICODE_STRING_REMOTE {
    pub length: u16,
    pub maximum_length: u16,
    pub buffer: *mut u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FILETIME {
    pub dw_low_date_time: u32,
    pub dw_high_date_time: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RM_UNIQUE_PROCESS {
    pub dw_process_id: u32,
    pub process_start_time: FILETIME,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RM_PROCESS_INFO {
    pub process: RM_UNIQUE_PROCESS,
    pub str_app_name: [u16; 256],
    pub str_service_short_name: [u16; 64],
    pub application_type: u32,
    pub app_status: u32,
    pub ts_session_id: u32,
    pub b_restartable: i32,
}

// ─── 常量 ──────────────────────────────────────────────

pub const INVALID_HANDLE_VALUE: isize = -1;

// File access / share modes
pub const GENERIC_READ: u32 = 0x80000000;
pub const GENERIC_WRITE: u32 = 0x40000000;
pub const FILE_LIST_DIRECTORY: u32 = 0x0001;
pub const FILE_SHARE_NONE: u32 = 0x00000000;
pub const OPEN_EXISTING: u32 = 3;
pub const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;

// Windows error codes
pub const ERROR_SHARING_VIOLATION: u32 = 32;
pub const ERROR_ACCESS_DENIED: u32 = 5;

// Process access rights
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
pub const PROCESS_VM_READ: u32 = 0x0010;
pub const PROCESS_TERMINATE: u32 = 0x0001;

// NtQueryInformationProcess info classes
pub const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;

// NTSTATUS
pub const STATUS_SUCCESS: i32 = 0;

// Toolhelp32
pub const TH32CS_SNAPPROCESS: u32 = 0x00000002;
