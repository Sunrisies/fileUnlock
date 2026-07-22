// ─── 控制台彩色输出（跨平台）────────────────────────────

#[cfg(target_os = "windows")]
mod imp {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(nStdHandle: u32) -> isize;
        fn SetConsoleTextAttribute(hConsoleOutput: isize, wAttributes: u16) -> i32;
    }

    const INVALID_HANDLE_VALUE: isize = -1;
    const STD_OUTPUT_HANDLE: u32 = 0xfffffff5;
    const FOREGROUND_RED: u16 = 4;
    const FOREGROUND_GREEN: u16 = 2;
    const FOREGROUND_BLUE: u16 = 1;
    const FOREGROUND_INTENSITY: u16 = 8;

    fn set_color(red: bool, green: bool, blue: bool, intensity: bool) {
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if handle == INVALID_HANDLE_VALUE || handle == 0 { return; }
            let mut attr = FOREGROUND_RED | FOREGROUND_GREEN | FOREGROUND_BLUE;
            if red   { attr |= FOREGROUND_RED; }   else { attr &= !FOREGROUND_RED; }
            if green { attr |= FOREGROUND_GREEN; } else { attr &= !FOREGROUND_GREEN; }
            if blue  { attr |= FOREGROUND_BLUE; }  else { attr &= !FOREGROUND_BLUE; }
            if intensity { attr |= FOREGROUND_INTENSITY; }
            SetConsoleTextAttribute(handle, attr);
        }
    }

    pub fn reset() { set_color(true, true, true, false); }
    pub fn green()  { set_color(false, true, false, true); }
    pub fn red()    { set_color(true, false, false, true); }
    pub fn yellow() { set_color(true, true, false, true); }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn reset()  { print!("\x1b[0m"); }
    pub fn green()  { print!("\x1b[1;32m"); }
    pub fn red()    { print!("\x1b[1;31m"); }
    pub fn yellow() { print!("\x1b[1;33m"); }
}

pub fn print_green(text: &str) {
    imp::green();
    print!("{text}");
    imp::reset();
}

pub fn print_red(text: &str) {
    imp::red();
    print!("{text}");
    imp::reset();
}

pub fn print_yellow(text: &str) {
    imp::yellow();
    print!("{text}");
    imp::reset();
}
