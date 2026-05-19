mod config;
mod api;

use rustyline::history::DefaultHistory;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{Validator, ValidationResult, ValidationContext};
use rustyline::{Context, Helper, Result as RustyResult};
use rustyline::error::ReadlineError;
use std::borrow::Cow;
use std::env;

use crate::api::{BaiduApiClient, ensure_valid_token, print_file_list, print_keyword_search_results, print_semantic_search_results};
use crate::config::Config;
use std::path::Path;
use std::fs;
use std::os::unix::fs::MetadataExt;

const COMMANDS: &[&str] = &[
    "pwd", "lpwd", "quota",
    "ls", "lls",
    "cd", "lcd",
    "mkdir", "lmkdir",
    "search", "semsearch",
    "put", "get",
    "rename", "mv", "cp", "rm",
    "lmv", "lcp", "lrm",
    "clear",
    "exit", "quit", "bye",
];

#[derive(Debug)]
struct BftpHelper;

impl Completer for BftpHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> RustyResult<(usize, Vec<Pair>)> {
        let line_prefix = &line[..pos];
        let (start, word) = match line_prefix.rsplit_once(char::is_whitespace) {
            Some((_, w)) => (line_prefix.len() - w.len(), w),
            None => (0, line_prefix),
        };
        let matches: Vec<Pair> = COMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(word))
            .map(|cmd| Pair {
                display: cmd.to_string(),
                replacement: cmd.to_string(),
            })
            .collect();
        Ok((start, matches))
    }
}

impl Hinter for BftpHelper {
    type Hint = String;
    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        None
    }
}

impl Highlighter for BftpHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        Cow::Borrowed(prompt)
    }
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Borrowed(hint)
    }
}

impl Validator for BftpHelper {
    fn validate(&self, _ctx: &mut ValidationContext) -> RustyResult<ValidationResult> {
        Ok(ValidationResult::Valid(None))
    }
    fn validate_while_typing(&self) -> bool {
        false
    }
}

impl Helper for BftpHelper {}


/// 处理命令行参数，返回用户名和 token
fn handle_command_line_args(mut config: Config, args: &[String]) -> (Config, String, String) {
    let username;
    let token = if args.len() > 1 {
        // 第一种方式：btfp ${user}
        username = args[1].clone();
        println!("使用指定用户: {}", username);
        config.get_or_create_user_token(&username)
    } else {
        // 第二种方式：btfp
        println!("使用默认用户: {}", config.default);
        username = config.default.clone();
        config.get_or_create_default_token()
    };

    (config, token, username)
}

/// 解析并执行命令
async fn execute_command(line: &str, client: &mut BaiduApiClient, rl: &mut rustyline::Editor<BftpHelper, DefaultHistory>) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    let command = parts[0];

    match command {
        "pwd" => {
            // 显示远程当前目录
            println!("{}", client.get_current_remote_path());
        }
        "quota" => {
            // 显示网盘容量信息
            match client.get_capacity_info().await {
                Ok(info) => {
                    println!("总空间: {}", format_local_size(info.total));
                    println!("已使用: {}", format_local_size(info.used));
                    println!("免费容量:   {}", format_local_size(info.free));
                    if info.expire {
                        println!("注意: 7天内有容量到期");
                    }
                }
                Err(e) => eprintln!("获取容量信息失败: {:?}", e),
            }
        }
        "lpwd" => {
            // 显示本地当前目录
            println!("{}", client.get_current_local_path());
        }
        "ls" => {
            // 列出远程目录内容
            match client.get_remote_current_path_files_info().await {
                Ok(files_info) => {
                    print_file_list(&files_info);
                }
                Err(e) => {
                    eprintln!("获取远程文件列表失败: {:?}", e);
                }
            }
        }
        "mkdir" => {
            // 创建远程目录
            if parts.len() < 2 {
                eprintln!("用法: mkdir <目录名>");
                return;
            }
            let path = normalize_remote_path(client.get_current_remote_path(), parts[1]);
            match client.create_remote_dir(&path).await {
                Ok(result) => println!("创建远程目录成功: {}", result.path.as_deref().unwrap_or(&path)),
                Err(e) => eprintln!("创建远程目录失败: {:?}", e),
            }
        }
        "lmkdir" => {
            // 创建本地目录
            if parts.len() < 2 {
                eprintln!("用法: lmkdir <目录名>");
                return;
            }
            let path = resolve_local_path(client.get_current_local_path(), parts[1]);
            match fs::create_dir_all(&path) {
                Ok(()) => println!("创建本地目录成功: {}", path),
                Err(e) => eprintln!("创建本地目录失败: {}", e),
            }
        }
        "search" => {
            // 关键字搜索远程文件
            if parts.len() < 2 {
                eprintln!("用法: search <关键字> [-r] [目录]");
                return;
            }
            let mut recursion = false;
            let mut dir: Option<&str> = None;
            let mut key_parts: Vec<&str> = Vec::new();
            let mut i = 1;
            while i < parts.len() {
                if parts[i] == "-r" {
                    recursion = true;
                } else if parts[i].starts_with('/') {
                    dir = Some(parts[i]);
                } else {
                    key_parts.push(parts[i]);
                }
                i += 1;
            }
            let key = key_parts.join(" ");
            if key.is_empty() {
                eprintln!("用法: search <关键字> [-r] [目录]");
                return;
            }
            match client.search_files_by_keyword(&key, dir, recursion).await {
                Ok(results) => print_keyword_search_results(&results),
                Err(e) => eprintln!("搜索失败: {:?}", e),
            }
        }
        "semsearch" => {
            // 语义搜索远程文件
            if parts.len() < 2 {
                eprintln!("用法: semsearch <查询内容> [-t 0|1|2] [目录]");
                eprintln!("  -t 0: 关键字搜索 (默认)");
                eprintln!("  -t 1: 语义搜索");
                eprintln!("  -t 2: 自动 (查询>5字符使用语义)");
                return;
            }
            let mut search_type = 1i32;
            let mut dir: Option<&str> = None;
            let mut query_parts: Vec<&str> = Vec::new();
            let mut i = 1;
            while i < parts.len() {
                if parts[i] == "-t" {
                    if i + 1 < parts.len() {
                        search_type = parts[i + 1].parse().unwrap_or(1);
                        i += 1;
                    }
                } else if parts[i].starts_with('/') {
                    dir = Some(parts[i]);
                } else {
                    query_parts.push(parts[i]);
                }
                i += 1;
            }
            let query = query_parts.join(" ");
            if query.is_empty() {
                eprintln!("用法: semsearch <查询内容> [-t 0|1|2] [目录]");
                return;
            }
            match client.search_files_semantic(&query, search_type, dir).await {
                Ok(results) => print_semantic_search_results(&results),
                Err(e) => eprintln!("语义搜索失败: {:?}", e),
            }
        }
        "lls" => {
            // 列出本地目录内容
            let show_all = parts.len() > 1 && parts[1] == "-la";
            let path = client.get_current_local_path();
            match fs::read_dir(path) {
                Ok(entries) => {
                    let mut dirs: Vec<_> = Vec::new();
                    let mut files: Vec<_> = Vec::new();
                    for entry in entries {
                        if let Ok(entry) = entry {
                            let name = entry.file_name().to_string_lossy().to_string();
                            // 如果没指定 -la，跳过隐藏文件（以.开头的文件）
                            if !show_all && name.starts_with('.') {
                                continue;
                            }
                            if let Ok(metadata) = entry.metadata() {
                                let file_type = if metadata.is_dir() { 'd' } else { '-' };
                                let size = metadata.len();
                                let size_str = format_local_size(size);
                                let time_str = format_local_timestamp(
                                    metadata.mtime() as u64
                                );
                                if metadata.is_dir() {
                                    dirs.push((file_type, size_str, time_str, name));
                                } else {
                                    files.push((file_type, size_str, time_str, name));
                                }
                            }
                        }
                    }
                    // 先显示目录，再显示文件
                    for (ft, sz, tm, name) in &dirs {
                        println!("{} {:>8} {} {}", ft, sz, tm, name);
                    }
                    for (ft, sz, tm, name) in &files {
                        println!("{} {:>8} {} {}", ft, sz, tm, name);
                    }
                }
                Err(e) => {
                    eprintln!("读取本地目录失败: {}", e);
                }
            }
        }
        "cd" => {
            // 切换远程目录
            if parts.len() < 2 {
                eprintln!("用法: cd <远程路径>");
                return;
            }
            let path = parts[1..].join(" ");
            let new_path = normalize_remote_path(client.get_current_remote_path(), &path);
            client.set_current_remote_path(new_path);
        }
        "lcd" => {
            // 切换本地目录
            if parts.len() < 2 {
                eprintln!("用法: lcd <本地路径>");
                return;
            }
            let path = parts[1..].join(" ");
            let new_path = normalize_local_path(client.get_current_local_path(), &path);
            // 验证本地目录是否存在
            if Path::new(&new_path).exists() {
                if let Err(e) = std::env::set_current_dir(&new_path) {
                    eprintln!("切换本地目录失败: {}", e);
                    return;
                }
                client.set_current_local_path(new_path);
            } else {
                eprintln!("本地目录不存在: {}", new_path);
            }
        }
        "put" => {
            if parts.len() < 2 {
                eprintln!("用法: put <本地文件> [远程文件名]");
                return;
            }
            let local_path = resolve_local_path(client.get_current_local_path(), parts[1]);
            let remote_filename = if parts.len() > 2 { Some(parts[2]) } else { None };
            if let Err(e) = client.upload_file(&local_path, remote_filename).await {
                eprintln!("上传失败: {}", e);
            }
        }
        "get" => {
            if parts.len() < 2 {
                eprintln!("用法: get [-r] <远程文件> [本地路径]");
                return;
            }
            // get -r remotedir [localdir]
            if parts[1] == "-r" {
                if parts.len() < 3 {
                    eprintln!("用法: get -r <远程目录> [本地目录]");
                    return;
                }
                let remote_dir = normalize_remote_path(client.get_current_remote_path(), parts[2]);
                let local_dir = if parts.len() > 3 {
                    resolve_local_path(client.get_current_local_path(), parts[3])
                } else {
                    let dirname = Path::new(&remote_dir)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("download");
                    resolve_local_path(client.get_current_local_path(), dirname)
                };
                if let Err(e) = client.download_dir(&remote_dir, &local_dir).await {
                    eprintln!("下载失败: {}", e);
                }
            } else {
                let remote_path = normalize_remote_path(client.get_current_remote_path(), parts[1]);
                let local_path = if parts.len() > 2 {
                    resolve_local_path(client.get_current_local_path(), parts[2])
                } else {
                    let filename = Path::new(&remote_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("download");
                    resolve_local_path(client.get_current_local_path(), filename)
                };
                if let Err(e) = client.download_file(&remote_path, &local_path).await {
                    eprintln!("下载失败: {}", e);
                }
            }
        }
        "rename" => {
            if parts.len() < 3 {
                eprintln!("用法: rename <远程文件> <新文件名>");
                return;
            }
            let path = normalize_remote_path(client.get_current_remote_path(), parts[1]);
            let newname = parts[2];
            if let Err(e) = client.rename_file(&path, newname).await {
                eprintln!("重命名失败: {}", e);
            } else {
                println!("重命名成功: {} -> {}", path, newname);
            }
        }
        "mv" => {
            if parts.len() < 3 {
                eprintln!("用法: mv <源文件> <目标路径>");
                return;
            }
            let src = normalize_remote_path(client.get_current_remote_path(), parts[1]);
            let dest_arg = parts[2];
            // 解析目标路径：获取目标目录和新文件名
            let (dest_dir, newname) = if dest_arg.ends_with('/') {
                let dir = normalize_remote_path(client.get_current_remote_path(), dest_arg);
                let name = Path::new(&src)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                (dir, name.to_string())
            } else {
                let full_dest = normalize_remote_path(client.get_current_remote_path(), dest_arg);
                let parent = Path::new(&full_dest)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("/");
                let name = Path::new(&full_dest)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                (parent.to_string(), name.to_string())
            };
            // 判断是否同目录：同目录直接用 rename，否则先 cp 再 rm
            let src_dir = Path::new(&src)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("/");
            if src_dir == dest_dir {
                if let Err(e) = client.rename_file(&src, &newname).await {
                    eprintln!("移动失败: {}", e);
                } else {
                    println!("移动成功: {} -> {}/{}", src, dest_dir, newname);
                }
            } else {
                if let Err(e) = client.copy_file(&src, &dest_dir, &newname).await {
                    eprintln!("移动失败(复制阶段): {}", e);
                    return;
                }
                if let Err(e) = client.delete_file(&src).await {
                    eprintln!("移动警告: 文件已复制到 {}/{}，但删除源文件失败: {}", dest_dir, newname, e);
                } else {
                    println!("移动成功: {} -> {}/{}", src, dest_dir, newname);
                }
            }
        }
        "cp" => {
            if parts.len() < 3 {
                eprintln!("用法: cp <源文件> <目标路径>");
                return;
            }
            let src = normalize_remote_path(client.get_current_remote_path(), parts[1]);
            let dest_arg = parts[2];
            // 将 dest_arg 解析为目录 + 新文件名
            let (dest_dir, newname) = if dest_arg.ends_with('/') {
                let dir = normalize_remote_path(client.get_current_remote_path(), dest_arg);
                let name = Path::new(&src)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                (dir, name.to_string())
            } else {
                let full_dest = normalize_remote_path(client.get_current_remote_path(), dest_arg);
                let parent = Path::new(&full_dest)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("/");
                let name = Path::new(&full_dest)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                (parent.to_string(), name.to_string())
            };
            if let Err(e) = client.copy_file(&src, &dest_dir, &newname).await {
                eprintln!("复制失败: {}", e);
            } else {
                println!("复制成功: {} -> {}/{}", src, dest_dir, newname);
            }
        }
        "rm" => {
            if parts.len() < 2 {
                eprintln!("用法: rm <远程文件>");
                return;
            }
            let path = normalize_remote_path(client.get_current_remote_path(), parts[1]);
            if let Err(e) = client.delete_file(&path).await {
                eprintln!("删除失败: {}", e);
            } else {
                println!("删除成功: {}", path);
            }
        }
        "lcp" => {
            if parts.len() < 3 {
                eprintln!("用法: lcp <源文件> <目标路径>");
                return;
            }
            let src = resolve_local_path(client.get_current_local_path(), parts[1]);
            let dest_arg = parts[2];
            let dest = if dest_arg.ends_with('/') {
                let dir = resolve_local_path(client.get_current_local_path(), dest_arg);
                let name = Path::new(&src)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                format!("{}/{}", dir, name)
            } else {
                resolve_local_path(client.get_current_local_path(), dest_arg)
            };
            if let Err(e) = fs::copy(&src, &dest) {
                eprintln!("本地复制失败: {}", e);
            } else {
                println!("复制成功: {} -> {}", src, dest);
            }
        }
        "lmv" => {
            if parts.len() < 3 {
                eprintln!("用法: lmv <源文件> <目标路径>");
                return;
            }
            let src = resolve_local_path(client.get_current_local_path(), parts[1]);
            let dest_arg = parts[2];
            let dest = if dest_arg.ends_with('/') {
                let dir = resolve_local_path(client.get_current_local_path(), dest_arg);
                let name = Path::new(&src)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                format!("{}/{}", dir, name)
            } else {
                resolve_local_path(client.get_current_local_path(), dest_arg)
            };
            match fs::rename(&src, &dest) {
                Ok(()) => println!("移动成功: {} -> {}", src, dest),
                Err(_) => {
                    // 跨文件系统时 rename 可能失败，尝试 cp + rm
                    if let Err(e2) = fs::copy(&src, &dest) {
                        eprintln!("移动失败: {}", e2);
                    } else if let Err(e3) = fs::remove_file(&src) {
                        eprintln!("移动警告: 文件已复制到 {}，但删除源文件失败: {}", dest, e3);
                    } else {
                        println!("移动成功: {} -> {}", src, dest);
                    }
                }
            }
        }
        "lrm" => {
            if parts.len() < 2 {
                eprintln!("用法: lrm <本地文件>");
                return;
            }
            let path = resolve_local_path(client.get_current_local_path(), parts[1]);
            if let Err(e) = fs::remove_file(&path) {
                eprintln!("删除失败: {}", e);
            } else {
                println!("删除成功: {}", path);
            }
        }
        "clear" => {
            // 清空控制台
            print!("\x1B[2J\x1B[1;1H");
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
        "exit" | "quit" | "bye" => {
            println!("bye");
            std::process::exit(0);
        }
        _ => {
            println!("未知命令: {}", command);
        }
    }
}

/// 格式化本地文件大小为可读形式
fn format_local_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut size = size as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{}B", size as u64)
    } else {
        format!("{:.1}{}", size, UNITS[unit_idx])
    }
}

/// 格式化本地文件时间戳
fn format_local_timestamp(timestamp: u64) -> String {
    let secs = timestamp as i64;
    let days_since_epoch = secs / 86400;
    let time_in_day = secs % 86400;

    let hours = time_in_day / 3600;
    let minutes = (time_in_day % 3600) / 60;

    let mut remaining_days = days_since_epoch;
    let mut year = 1970i64;
    let mut month = 1u32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days >= days_in_year {
            remaining_days -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for (i, &days) in days_in_months.iter().enumerate() {
        if remaining_days >= days {
            remaining_days -= days;
        } else {
            month = (i + 1) as u32;
            break;
        }
    }

    let day = (remaining_days + 1) as u32;

    format!("{:02}-{:02} {:02}:{:02}", month, day, hours, minutes)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// 规范化远程路径（百度网盘路径）
fn normalize_remote_path(current: &str, target: &str) -> String {
    if target.starts_with('/') {
        // 绝对路径
        let path = target.to_string();
        simplify_path(&path)
    } else if target == ".." {
        // 上级目录
        let parent = if current == "/" {
            "/"
        } else {
            let trimmed = current.trim_end_matches('/');
            match trimmed.rfind('/') {
                Some(pos) => {
                    if pos == 0 {
                        "/"
                    } else {
                        &trimmed[..pos]
                    }
                },
                None => "/",
            }
        };
        parent.to_string()
    } else if target == "." {
        current.to_string()
    } else {
        // 相对路径
        let new_path = if current.ends_with('/') {
            format!("{}{}", current, target)
        } else {
            format!("{}/{}", current, target)
        };
        simplify_path(&new_path)
    }
}

/// 规范化本地路径
fn normalize_local_path(current: &str, target: &str) -> String {
    if target.starts_with('/') {
        // 绝对路径
        let path = target.to_string();
        simplify_path(&path)
    } else if target == ".." {
        // 上级目录
        let parent = if current == "/" {
            "/"
        } else {
            let trimmed = current.trim_end_matches('/');
            match trimmed.rfind('/') {
                Some(pos) => {
                    if pos == 0 {
                        "/"
                    } else {
                        &trimmed[..pos]
                    }
                },
                None => "/",
            }
        };
        parent.to_string()
    } else if target == "." {
        current.to_string()
    } else {
        // 相对路径
        let new_path = if current.ends_with('/') {
            format!("{}{}", current, target)
        } else {
            format!("{}/{}", current, target)
        };
        simplify_path(&new_path)
    }
}

/// 简化路径（去除多余的 / 和 ..）
fn simplify_path(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                if !stack.is_empty() {
                    stack.pop();
                }
            }
            _ => stack.push(component),
        }
    }
    if stack.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stack.join("/"))
    }
}

/// 解析本地文件路径（支持相对路径和绝对路径）
fn resolve_local_path(current_local_path: &str, target: &str) -> String {
    if target.starts_with('/') {
        target.to_string()
    } else {
        if current_local_path.ends_with('/') {
            format!("{}{}", current_local_path, target)
        } else {
            format!("{}/{}", current_local_path, target)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    // 加载配置
    let config = match Config::load_default() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("加载配置失败: {}", e);
            return Ok(());
        }
    };
    // 验证配置
    if let Err(e) = config.validate() {
        eprintln!("配置验证失败: {}", e);
        return Ok(());
    }
    // 处理命令行参数，获取 token
    let (mut config, _token, username) = handle_command_line_args(config, &args);
    let token = ensure_valid_token(&mut config, &username).await?;

    let mut client = BaiduApiClient::new(token);
    let userinfo = client.get_user_info().await?;
    println!("baidu_name: {:?} ",userinfo.baidu_name.unwrap());
    // `()` can be used when no completer is required
    let mut rl = rustyline::Editor::<BftpHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(BftpHelper));

    loop {
        let prompt = format!("\x1b[1;36m{}\x1b[0m:\x1b[1;32m{}\x1b[0m> ",
            username, client.get_current_remote_path());
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                execute_command(&line, &mut client, &mut rl).await;
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}
