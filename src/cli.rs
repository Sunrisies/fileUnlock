use std::path::Path;

pub fn print_usage(prog: &str) {
    let prog = Path::new(prog)
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed(prog));

    eprintln!("FileUnlock v1.0 — 检测并安全操作被占用的文件/文件夹 (Windows)");
    eprintln!();
    eprintln!("用法:");
    eprintln!("  {prog} check   <路径>             检查文件是否被占用");
    eprintln!("  {prog} delete  <路径>             安全删除（先检查）");
    eprintln!("  {prog} rename  <源路径> <目标>     安全重命名/移动（先检查）");
    eprintln!("  {prog} move    <源路径> <目标>     同上");
    eprintln!("  {prog} ps      <进程名>           搜索正在运行的进程");
    eprintln!("  {prog} kill    <PID/进程名>       结束指定进程");
    eprintln!("  {prog} port    <端口号>           查找占用指定端口的进程（显示命令行）");
    eprintln!("  {prog} where   <程序名>           在 PATH 中查找程序位置");
    eprintln!();
    eprintln!("中文别名（等价）:");
    eprintln!("  check  = 检查");
    eprintln!("  delete = 删除");
    eprintln!("  rename = 重命名");
    eprintln!("  move   = 移动");
    eprintln!("  ps     = 进程");
    eprintln!("  kill   = 结束");
    eprintln!("  port   = 端口");
    eprintln!("  where  = 查找");
    eprintln!("  which  = 查找");
    eprintln!();
    eprintln!("例子:");
    eprintln!("  {prog} 检查 Cargo.toml");
    eprintln!("  {prog} 删除 D:\\锁定文件.txt");
    eprintln!("  {prog} 重命名 old.txt new.txt");
    eprintln!("  {prog} move   源文件.exe D:\\备份\\");
    eprintln!("  {prog} kill   61928");
    eprintln!("  {prog} where  node");
    eprintln!("  {prog} 查找   notepad");
    eprintln!();
    eprintln!("参数:");
    eprintln!("  -h, --help    显示此帮助信息");
    eprintln!();
    eprintln!("退出码:");
    eprintln!("  0  操作成功，或文件未被占用");
    eprintln!("  1  文件正被占用，操作被拒绝");
    eprintln!("  2  路径不存在或其他错误");
}
