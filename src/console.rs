// ─── 控制台彩色输出 ─────────────────────────────────────

// Windows API FFI
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(nStdHandle: u32) -> isize;
    fn SetConsoleTextAttribute(hConsoleOutput: isize, wAttributes: u16) -> i32;
}

const INVALID_HANDLE_VALUE: isize = -1;
const STD_OUTPUT_HANDLE: u32 = 0xfffffff5;

// Console colors
const FOREGROUND_RED: u16 = 4;
const FOREGROUND_GREEN: u16 = 2;
const FOREGROUND_BLUE: u16 = 1;
const FOREGROUND_INTENSITY: u16 = 8;

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

pub fn print_green(text: &str) {
    set_console_color(false, true, false, true);
    print!("{}", text);
    reset_color();
}

pub fn print_red(text: &str) {
    set_console_color(true, false, false, true);
    print!("{}", text);
    reset_color();
}

pub fn print_yellow(text: &str) {
    set_console_color(true, true, false, true);
    print!("{}", text);
    reset_color();
}
