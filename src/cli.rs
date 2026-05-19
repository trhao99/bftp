use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{Validator, ValidationResult, ValidationContext};
use rustyline::{Context, Helper, Result as RustyResult};
use rustyline::history::DefaultHistory;
use std::borrow::Cow;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::client::BaiduApiClient;
use crate::display::{
    format_size, format_timestamp, print_file_list, print_keyword_search_results,
    print_semantic_search_results,
};
use crate::config::Config;

const COMMANDS: &[&str] = &[
    "help",
    "pwd", "lpwd", "quota",
    "ls", "lls",
    "cd", "lcd",
    "mkdir", "lmkdir",
    "search", "semsearch",
    "put", "get", "mget",
    "rename", "mv", "cp", "rm",
    "lmv", "lcp", "lrm",
    "clear",
    "exit", "quit", "bye",
];

const HELP_TEXT: &str = "\
\x1b[1m导航命令:\x1b[0m
  pwd              显示远程当前目录
  lpwd             显示本地当前目录
  cd <路径>         切换远程目录
  lcd <路径>        切换本地目录
  ls               列出远程目录
  lls [-la]        列出本地目录
  quota            显示网盘容量信息
  clear            清空控制台

\x1b[1m远程文件操作:\x1b[0m
  mkdir <目录>      创建远程目录
  rename <文件> <新名>  重命名远程文件
  mv <源> <目标>    移动远程文件
  cp <源> <目标>    复制远程文件
  rm <文件>         删除远程文件

\x1b[1m本地文件操作:\x1b[0m
  lmkdir <目录>     创建本地目录
  lmv <源> <目标>   移动本地文件
  lcp <源> <目标>   复制本地文件
  lrm <文件>        删除本地文件

\x1b[1m文件传输:\x1b[0m
  put <本地文件> [远程名]    上传文件
  get <远程文件> [本地路径]  下载文件
  get -r <远程目录> [本地]   递归下载目录
  mget <远程文件> [本地路径] 多线程下载文件（默认4线程）
  mget -t N <远程文件> [本地] 多线程下载（N线程）
  mget -r <远程目录> [本地]  多线程递归下载目录

\x1b[1m文件搜索:\x1b[0m
  search <关键字> [-r] [目录]     关键字搜索
  semsearch <查询> [-t 0|1|2] [目录]  语义搜索

\x1b[1m连接:\x1b[0m
  exit / quit / bye  退出程序

\x1b[1m配置管理:\x1b[0m
  bftp config [show|add-user|remove-user|set-default|list-users]";

#[derive(Debug)]
pub struct BftpHelper;

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
pub fn handle_command_line_args(mut config: Config, args: &[String]) -> (Config, String, String) {
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
pub async fn execute_command(
    line: &str,
    client: &mut BaiduApiClient,
    _rl: &mut rustyline::Editor<BftpHelper, DefaultHistory>,
) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    let command = parts[0];

    match command {
        "help" => {
            println!("{}", HELP_TEXT);
        }
        "pwd" => {
            // 显示远程当前目录
            println!("{}", client.get_current_remote_path());
        }
        "quota" => {
            // 显示网盘容量信息
            match client.get_capacity_info().await {
                Ok(info) => {
                    println!("总空间: {}", format_size(info.total));
                    println!("已使用: {}", format_size(info.used));
                    println!("免费容量:   {}", format_size(info.free));
                    if info.expire {
                        println!("注意: 7天内有容量到期");
                    }
                }
                Err(e) => eprintln!("获取容量信息失败: {}", e),
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
                    eprintln!("获取远程文件列表失败: {}", e);
                }
            }
        }
        "mkdir" => {
            // 创建远程目录
            if parts.len() < 2 {
                eprintln!("用法: mkdir <目录名>");
                return;
            }
            let path = normalize_path(client.get_current_remote_path(), parts[1]);
            match client.create_remote_dir(&path).await {
                Ok(result) => println!("创建远程目录成功: {}", result.path.as_deref().unwrap_or(&path)),
                Err(e) => eprintln!("创建远程目录失败: {}", e),
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
                Err(e) => eprintln!("搜索失败: {}", e),
            }
        }
        "semsearch" => {
            // 语义搜索远程文件
            if parts.len() < 2 {
                eprintln!("用法: semsearch <查询内容> [-t 0|1|2] [目录]");
                eprintln!("  -t 0: 关键字搜索");
                eprintln!("  -t 1: 语义搜索（默认）");
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
                Err(e) => eprintln!("语义搜索失败: {}", e),
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
                                let size_str = format_size(size);
                                let time_str = format_timestamp(metadata.mtime() as u64);
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
            let new_path = normalize_path(client.get_current_remote_path(), &path);
            client.set_current_remote_path(new_path);
        }
        "lcd" => {
            // 切换本地目录
            if parts.len() < 2 {
                eprintln!("用法: lcd <本地路径>");
                return;
            }
            let path = parts[1..].join(" ");
            let new_path = normalize_path(client.get_current_local_path(), &path);
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
            let remote_filename = if parts.len() > 2 {
                Some(parts[2])
            } else {
                None
            };
            if let Err(e) = client.upload_file(&local_path, remote_filename).await {
                eprintln!("上传失败: {}", e);
            }
        }
        "get" => {
            if parts.len() < 2 {
                eprintln!("用法: get [-r] <远程文件> [本地路径]");
                return;
            }
            if parts[1] == "-r" {
                if parts.len() < 3 {
                    eprintln!("用法: get -r <远程目录> [本地目录]");
                    return;
                }
                let remote_dir = normalize_path(client.get_current_remote_path(), parts[2]);
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
                let remote_path = normalize_path(client.get_current_remote_path(), parts[1]);
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
        "mget" => {
            if parts.len() < 2 {
                eprintln!("用法: mget [-r] [-t 线程数] <远程文件> [本地路径]");
                return;
            }
            let mut recursive = false;
            let mut num_threads = 4usize;
            let mut args_start = 1;

            while args_start < parts.len() {
                match parts[args_start] {
                    "-r" => {
                        recursive = true;
                        args_start += 1;
                    }
                    "-t" => {
                        if args_start + 1 < parts.len() {
                            num_threads = parts[args_start + 1].parse().unwrap_or(4);
                            args_start += 2;
                        } else {
                            eprintln!("用法: mget -t <线程数>");
                            return;
                        }
                    }
                    _ => break,
                }
            }

            if args_start >= parts.len() {
                eprintln!("用法: mget [-r] [-t 线程数] <远程文件> [本地路径]");
                return;
            }

            if recursive {
                let remote_dir = normalize_path(client.get_current_remote_path(), parts[args_start]);
                let local_dir = if args_start + 1 < parts.len() {
                    resolve_local_path(client.get_current_local_path(), parts[args_start + 1])
                } else {
                    let dirname = Path::new(&remote_dir)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("download");
                    resolve_local_path(client.get_current_local_path(), dirname)
                };
                if let Err(e) = client.download_dir_mt(&remote_dir, &local_dir, num_threads).await {
                    eprintln!("下载失败: {}", e);
                }
            } else {
                let remote_path = normalize_path(client.get_current_remote_path(), parts[args_start]);
                let local_path = if args_start + 1 < parts.len() {
                    resolve_local_path(client.get_current_local_path(), parts[args_start + 1])
                } else {
                    let filename = Path::new(&remote_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("download");
                    resolve_local_path(client.get_current_local_path(), filename)
                };
                if let Err(e) = client.download_file_mt(&remote_path, &local_path, num_threads).await {
                    eprintln!("下载失败: {}", e);
                }
            }
        }
        "rename" => {
            if parts.len() < 3 {
                eprintln!("用法: rename <远程文件> <新文件名>");
                return;
            }
            let path = normalize_path(client.get_current_remote_path(), parts[1]);
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
            let src = normalize_path(client.get_current_remote_path(), parts[1]);
            let dest_arg = parts[2];
            let (dest_dir, newname) = parse_dest_path(client.get_current_remote_path(), dest_arg, &src);
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
            let src = normalize_path(client.get_current_remote_path(), parts[1]);
            let dest_arg = parts[2];
            let (dest_dir, newname) =
                parse_dest_path(client.get_current_remote_path(), dest_arg, &src);
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
            let path = normalize_path(client.get_current_remote_path(), parts[1]);
            if !confirm_delete(&path) {
                println!("已取消");
                return;
            }
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
            if !confirm_delete(&path) {
                println!("已取消");
                return;
            }
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

/// 规范化路径（支持远程和本地路径）
fn normalize_path(current: &str, target: &str) -> String {
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
                }
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

/// 解析目标路径，返回 (目录, 新文件名)
fn parse_dest_path(current: &str, dest: &str, src: &str) -> (String, String) {
    let src_name = Path::new(src)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("untitled");
    if dest.ends_with('/') {
        let dir = normalize_path(current, dest);
        (dir, src_name.to_string())
    } else {
        let full_dest = normalize_path(current, dest);
        let parent = Path::new(&full_dest)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("/");
        let name = Path::new(&full_dest)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("untitled");
        (parent.to_string(), name.to_string())
    }
}

/// 删除确认提示，返回 true 表示确认删除
fn confirm_delete(path: &str) -> bool {
    use std::io::{self, Write};
    print!("确认删除 {}? (y/N): ", path);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
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

/// 打印当前配置信息
fn print_config(config: &Config) {
    println!("当前配置:");
    println!("  client_id:    {}", config.client_id);
    println!("  redirect_uri: {}", config.redirect_uri);
    println!("  scope:        {}", config.scope);
    println!("  默认用户:     {}", config.default);
    println!("  用户列表:");
    for user in config.get_users() {
        let has_token = config.get_user_token(user).map_or(false, |t| !t.is_empty());
        let status = if has_token { "已授权" } else { "未授权" };
        if *user == config.default {
            println!("    * {} ({})", user, status);
        } else {
            println!("    - {} ({})", user, status);
        }
    }
}

/// 处理 bftp config 子命令
pub fn handle_config_command(mut config: Config, args: &[String]) -> anyhow::Result<()> {
    if args.len() <= 2 {
        print_config(&config);
        return Ok(());
    }

    match args[2].as_str() {
        "show" => print_config(&config),
        "set-default" => {
            if args.len() < 4 {
                eprintln!("用法: bftp config set-default <用户名>");
                return Ok(());
            }
            let username = &args[3];
            if !config.has_user(username) {
                eprintln!("用户 '{}' 不存在", username);
                return Ok(());
            }
            config.set_default_user(username);
            config.save_default()?;
            println!("默认用户已切换为: {}", username);
        }
        "add-user" => {
            if args.len() < 4 {
                eprintln!("用法: bftp config add-user <用户名>");
                return Ok(());
            }
            let username = &args[3];
            config.add_user(username, None);
            config.save_default()?;
            println!("用户 '{}' 已添加", username);
        }
        "remove-user" => {
            if args.len() < 4 {
                eprintln!("用法: bftp config remove-user <用户名>");
                return Ok(());
            }
            let username = &args[3];
            if config.remove_user(username).is_none() {
                eprintln!("用户 '{}' 不存在", username);
                return Ok(());
            }
            config.save_default()?;
            println!("用户 '{}' 已删除", username);
        }
        "list-users" => {
            println!("用户列表:");
            for user in config.get_users() {
                if *user == config.default {
                    println!("  * {} (默认)", user);
                } else {
                    println!("  - {}", user);
                }
            }
        }
        _ => {
            eprintln!("未知子命令: {}", args[2]);
            eprintln!("可用子命令: show, set-default, add-user, remove-user, list-users");
        }
    }
    Ok(())
}
