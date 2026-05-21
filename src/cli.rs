use rustyline::history::DefaultHistory;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::client::BaiduApiClient;
use crate::config::Config;
use crate::constants::DEFAULT_THREADS;
use crate::display::{
    format_size, format_timestamp, print_file_list, print_keyword_search_results,
    print_semantic_search_results,
};
use crate::path_utils::{normalize_path, parse_dest_path, resolve_local_path};
use crate::wildcard::{
    expand_local_wildcard_all, expand_local_wildcard_files, expand_remote_wildcard_files,
    has_wildcards, parse_wildcard_path, remove_local_path,
};

pub use crate::repl_helper::BftpHelper;

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
  rm <文件>         删除远程文件（支持通配符: rm *.txt）

\x1b[1m本地文件操作:\x1b[0m
  lmkdir <目录>     创建本地目录
  lmv <源> <目标>   移动本地文件
  lcp <源> <目标>   复制本地文件
  lrm <文件>        删除本地文件（支持通配符: lrm *.tmp）

\x1b[1m文件传输:\x1b[0m
  put <本地文件> [远程名]    上传文件（支持通配符: put *.txt）
  get <远程文件> [本地路径]  下载文件（支持通配符: get *.txt）
  get -r <远程目录> [本地]   递归下载目录（支持通配符: get -r dir*）
  mget <远程文件> [本地路径] 多线程下载文件（默认4线程，支持通配符）
  mget -t N <远程文件> [本地] 多线程下载（N线程）
  mget -r <远程目录> [本地]  多线程递归下载目录（支持通配符）

\x1b[1m文件搜索:\x1b[0m
  search <关键字> [-r] [目录]     关键字搜索
  semsearch <查询> [-t 0|1|2] [目录]  语义搜索

\x1b[1m连接:\x1b[0m
  exit / quit / bye  退出程序

\x1b[1m配置管理:\x1b[0m
  bftp config [show|add-user|remove-user|set-default|list-users]";

// ---- 命令行参数处理 ----

pub fn handle_command_line_args(mut config: Config, args: &[String]) -> (Config, String, String) {
    let username;
    let token = if args.len() > 1 {
        username = args[1].clone();
        println!("使用指定用户: {}", username);
        config.get_or_create_user_token_mut(&username)
    } else {
        username = config.default.clone();
        println!("使用默认用户: {}", username);
        config.get_or_create_user_token_mut(&username)
    };
    (config, token, username)
}

// ---- 命令分发 ----

pub async fn execute_command(
    line: &str,
    client: &mut BaiduApiClient,
    _rl: &mut rustyline::Editor<BftpHelper, DefaultHistory>,
) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }

    let parts = shell_split(line);
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let command = parts[0];

    match command {
        "help" => println!("{}", HELP_TEXT),
        "pwd" => cmd_pwd(client),
        "lpwd" => cmd_lpwd(client),
        "quota" => cmd_quota(client).await,
        "ls" => cmd_ls(client).await,
        "lls" => cmd_lls(client, &parts),
        "cd" => cmd_cd(client, &parts),
        "lcd" => cmd_lcd(client, &parts),
        "mkdir" => cmd_mkdir(client, &parts).await,
        "lmkdir" => cmd_lmkdir(client, &parts),
        "search" => cmd_search(client, &parts).await,
        "semsearch" => cmd_semsearch(client, &parts).await,
        "put" => cmd_put(client, &parts).await,
        "get" => cmd_get(client, &parts, false, None).await,
        "mget" => cmd_mget(client, &parts).await,
        "rename" => cmd_rename(client, &parts).await,
        "mv" => cmd_mv(client, &parts).await,
        "cp" => cmd_cp(client, &parts).await,
        "rm" => cmd_rm(client, &parts).await,
        "lcp" => cmd_lcp(client, &parts),
        "lmv" => cmd_lmv(client, &parts),
        "lrm" => cmd_lrm(client, &parts),
        "clear" => cmd_clear(),
        "exit" | "quit" | "bye" => {
            println!("bye");
            std::process::exit(0);
        }
        _ => println!("未知命令: {}", command),
    }
}

// ---- 导航命令 ----

fn cmd_pwd(client: &BaiduApiClient) {
    println!("{}", &client.session.current_remote_path);
}

fn cmd_lpwd(client: &BaiduApiClient) {
    println!("{}", &client.session.current_local_path);
}

async fn cmd_quota(client: &BaiduApiClient) {
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

async fn cmd_ls(client: &BaiduApiClient) {
    match client.get_remote_current_path_files_info().await {
        Ok(files_info) => print_file_list(&files_info),
        Err(e) => eprintln!("获取远程文件列表失败: {}", e),
    }
}

fn cmd_lls(client: &BaiduApiClient, parts: &[&str]) {
    let show_all = parts.len() > 1 && parts[1] == "-la";
    let path = &client.session.current_local_path;
    match fs::read_dir(path) {
        Ok(entries) => {
            let mut dirs: Vec<_> = Vec::new();
            let mut files: Vec<_> = Vec::new();
            for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
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
            for (ft, sz, tm, name) in &dirs {
                println!("{} {:>8} {} {}", ft, sz, tm, name);
            }
            for (ft, sz, tm, name) in &files {
                println!("{} {:>8} {} {}", ft, sz, tm, name);
            }
        }
        Err(e) => eprintln!("读取本地目录失败: {}", e),
    }
}

fn cmd_cd(client: &mut BaiduApiClient, parts: &[&str]) {
    if parts.len() < 2 {
        eprintln!("用法: cd <远程路径>");
        return;
    }
    let path = parts[1..].join(" ");
    let new_path = normalize_path(&client.session.current_remote_path, &path);
    client.session.current_remote_path = new_path;
}

fn cmd_lcd(client: &mut BaiduApiClient, parts: &[&str]) {
    if parts.len() < 2 {
        eprintln!("用法: lcd <本地路径>");
        return;
    }
    let path = parts[1..].join(" ");
    let new_path = normalize_path(&client.session.current_local_path, &path);
    if Path::new(&new_path).exists() {
        if let Err(e) = std::env::set_current_dir(&new_path) {
            eprintln!("切换本地目录失败: {}", e);
            return;
        }
        client.session.current_local_path = new_path;
    } else {
        eprintln!("本地目录不存在: {}", new_path);
    }
}

// ---- 远程文件操作 ----

async fn cmd_mkdir(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 2 {
        eprintln!("用法: mkdir <目录名>");
        return;
    }
    let path = normalize_path(&client.session.current_remote_path, parts[1]);
    match client.create_remote_dir(&path).await {
        Ok(result) => println!("创建远程目录成功: {}", result.path.as_deref().unwrap_or(&path)),
        Err(e) => eprintln!("创建远程目录失败: {}", e),
    }
}

fn cmd_lmkdir(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 2 {
        eprintln!("用法: lmkdir <目录名>");
        return;
    }
    let path = resolve_local_path(&client.session.current_local_path, parts[1]);
    match fs::create_dir_all(&path) {
        Ok(()) => println!("创建本地目录成功: {}", path),
        Err(e) => eprintln!("创建本地目录失败: {}", e),
    }
}

async fn cmd_search(client: &BaiduApiClient, parts: &[&str]) {
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

async fn cmd_semsearch(client: &BaiduApiClient, parts: &[&str]) {
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

async fn cmd_rename(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 3 {
        eprintln!("用法: rename <远程文件> <新文件名>");
        return;
    }
    let path = normalize_path(&client.session.current_remote_path, parts[1]);
    let newname = parts[2];
    if let Err(e) = client.rename_file(&path, newname).await {
        eprintln!("重命名失败: {}", e);
    } else {
        println!("重命名成功: {} -> {}", path, newname);
    }
}

async fn cmd_mv(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 3 {
        eprintln!("用法: mv <源文件> <目标路径>");
        return;
    }
    let src = normalize_path(&client.session.current_remote_path, parts[1]);
    let dest_arg = parts[2];
    let (dest_dir, newname) = parse_dest_path(&client.session.current_remote_path, dest_arg, &src);
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

async fn cmd_cp(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 3 {
        eprintln!("用法: cp <源文件> <目标路径>");
        return;
    }
    let src = normalize_path(&client.session.current_remote_path, parts[1]);
    let dest_arg = parts[2];
    let (dest_dir, newname) = parse_dest_path(&client.session.current_remote_path, dest_arg, &src);
    if let Err(e) = client.copy_file(&src, &dest_dir, &newname).await {
        eprintln!("复制失败: {}", e);
    } else {
        println!("复制成功: {} -> {}/{}", src, dest_dir, newname);
    }
}

async fn cmd_rm(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 2 {
        eprintln!("用法: rm <远程文件>");
        eprintln!("支持通配符: rm *.txt");
        return;
    }
    if has_wildcards(parts[1]) {
        let (dir, pattern) = parse_wildcard_path(parts[1], &client.session.current_remote_path, true);
        match expand_remote_wildcard_files(client, &dir, &pattern, false).await {
            Ok(paths) => {
                if paths.is_empty() {
                    println!("没有匹配的文件: {}", parts[1]);
                    return;
                }
                println!("匹配到 {} 个文件:", paths.len());
                for p in &paths {
                    println!("  {}", p);
                }
                if !confirm_delete_batch(paths.len()) {
                    println!("已取消");
                    return;
                }
                for path in &paths {
                    if let Err(e) = client.delete_file(path).await {
                        eprintln!("删除 {} 失败: {}", path, e);
                    } else {
                        println!("删除成功: {}", path);
                    }
                }
            }
            Err(e) => eprintln!("展开通配符失败: {}", e),
        }
    } else {
        let path = normalize_path(&client.session.current_remote_path, parts[1]);
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
}

// ---- 本地文件操作 ----

fn cmd_lcp(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 3 {
        eprintln!("用法: lcp <源文件> <目标路径>");
        eprintln!("支持通配符: lcp *.txt <目标目录>");
        return;
    }
    if has_wildcards(parts[1]) {
        let (dir, pattern) = parse_wildcard_path(parts[1], &client.session.current_local_path, false);
        let dest_dir = resolve_local_path(&client.session.current_local_path, parts[2]);
        match expand_local_wildcard_files(&dir, &pattern) {
            Ok(files) => {
                if files.is_empty() {
                    println!("没有匹配的文件: {}", parts[1]);
                    return;
                }
                println!("匹配到 {} 个文件", files.len());
                for src in &files {
                    let name = Path::new(src).file_name().and_then(|n| n.to_str()).unwrap_or("untitled");
                    let dest = format!("{}/{}", dest_dir, name);
                    if let Err(e) = fs::copy(src, &dest) {
                        eprintln!("复制 {} 失败: {}", src, e);
                    } else {
                        println!("复制成功: {} -> {}", src, dest);
                    }
                }
            }
            Err(e) => eprintln!("展开通配符失败: {}", e),
        }
    } else {
        let src = resolve_local_path(&client.session.current_local_path, parts[1]);
        let dest = resolve_local_dest(client, &src, parts[2]);
        if let Err(e) = fs::copy(&src, &dest) {
            eprintln!("本地复制失败: {}", e);
        } else {
            println!("复制成功: {} -> {}", src, dest);
        }
    }
}

fn cmd_lmv(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 3 {
        eprintln!("用法: lmv <源文件> <目标路径>");
        eprintln!("支持通配符: lmv *.txt <目标目录>");
        return;
    }
    if has_wildcards(parts[1]) {
        let (dir, pattern) = parse_wildcard_path(parts[1], &client.session.current_local_path, false);
        let dest_dir = resolve_local_path(&client.session.current_local_path, parts[2]);
        match expand_local_wildcard_files(&dir, &pattern) {
            Ok(files) => {
                if files.is_empty() {
                    println!("没有匹配的文件: {}", parts[1]);
                    return;
                }
                println!("匹配到 {} 个文件", files.len());
                for src in &files {
                    let name = Path::new(src).file_name().and_then(|n| n.to_str()).unwrap_or("untitled");
                    let dest = format!("{}/{}", dest_dir, name);
                    match fs::rename(src, &dest) {
                        Ok(()) => println!("移动成功: {} -> {}", src, dest),
                        Err(_) => {
                            if let Err(e2) = fs::copy(src, &dest) {
                                eprintln!("移动 {} 失败: {}", src, e2);
                            } else if let Err(e3) = fs::remove_file(src) {
                                eprintln!("移动警告: 已复制到 {}，但删除源文件 {} 失败: {}", dest, src, e3);
                            } else {
                                println!("移动成功: {} -> {}", src, dest);
                            }
                        }
                    }
                }
            }
            Err(e) => eprintln!("展开通配符失败: {}", e),
        }
    } else {
        let src = resolve_local_path(&client.session.current_local_path, parts[1]);
        let dest = resolve_local_dest(client, &src, parts[2]);
        match fs::rename(&src, &dest) {
            Ok(()) => println!("移动成功: {} -> {}", src, dest),
            Err(_) => {
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
}

fn cmd_lrm(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 2 {
        eprintln!("用法: lrm <本地文件或目录>");
        eprintln!("支持通配符: lrm *.txt");
        return;
    }
    if has_wildcards(parts[1]) {
        let (dir, pattern) = parse_wildcard_path(parts[1], &client.session.current_local_path, false);
        match expand_local_wildcard_all(&dir, &pattern) {
            Ok(entries) => {
                if entries.is_empty() {
                    println!("没有匹配的条目: {}", parts[1]);
                    return;
                }
                println!("匹配到 {} 个条目:", entries.len());
                for f in &entries {
                    println!("  {}", f);
                }
                if !confirm_delete_batch(entries.len()) {
                    println!("已取消");
                    return;
                }
                for path in &entries {
                    if let Err(e) = remove_local_path(path) {
                        eprintln!("删除 {} 失败: {}", path, e);
                    } else {
                        println!("删除成功: {}", path);
                    }
                }
            }
            Err(e) => eprintln!("展开通配符失败: {}", e),
        }
    } else {
        let path = resolve_local_path(&client.session.current_local_path, parts[1]);
        if !confirm_delete(&path) {
            println!("已取消");
            return;
        }
        if let Err(e) = remove_local_path(&path) {
            eprintln!("删除失败: {}", e);
        } else {
            println!("删除成功: {}", path);
        }
    }
}

// ---- 文件传输 ----

async fn cmd_put(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 2 {
        eprintln!("用法: put <本地文件> [远程文件名]");
        eprintln!("支持通配符: put *.txt");
        return;
    }
    if has_wildcards(parts[1]) {
        if parts.len() > 2 {
            eprintln!("通配符模式不支持指定远程文件名");
            return;
        }
        let (dir, pattern) = parse_wildcard_path(parts[1], &client.session.current_local_path, false);
        match expand_local_wildcard_files(&dir, &pattern) {
            Ok(files) => {
                if files.is_empty() {
                    println!("没有匹配的文件: {}", parts[1]);
                    return;
                }
                println!("匹配到 {} 个文件", files.len());
                for local_file in &files {
                    let name = Path::new(local_file)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    println!("上传: {} -> {}/{}", name, &client.session.current_remote_path, name);
                    if let Err(e) = client.upload_file(local_file, None, Some(&|s| println!("{}", s))).await {
                        eprintln!("上传 {} 失败: {}", local_file, e);
                    }
                }
            }
            Err(e) => eprintln!("展开通配符失败: {}", e),
        }
    } else {
        let local_path = resolve_local_path(&client.session.current_local_path, parts[1]);
        let remote_filename = if parts.len() > 2 { Some(parts[2]) } else { None };
        if let Err(e) = client.upload_file(&local_path, remote_filename, Some(&|s| println!("{}", s))).await {
            eprintln!("上传失败: {}", e);
        }
    }
}

/// 统一的 get 实现：recursive=false+num_threads=None=单线程；num_threads=Some=多线程；recursive=true=目录下载
async fn cmd_get(client: &BaiduApiClient, parts: &[&str], recursive: bool, num_threads: Option<usize>) {
    if parts.len() < 2 {
        eprintln!("用法: get [-r] <远程文件> [本地路径]");
        eprintln!("支持通配符: get *.txt  /  get -r dir*");
        return;
    }

    if recursive {
        if parts.len() < 3 {
            eprintln!("用法: get -r <远程目录> [本地目录]");
            return;
        }
        if has_wildcards(parts[2]) {
            let (dir, pattern) = parse_wildcard_path(parts[2], &client.session.current_remote_path, true);
            let local_base = if parts.len() > 3 {
                resolve_local_path(&client.session.current_local_path, parts[3])
            } else {
                client.session.current_local_path.clone()
            };
            match expand_remote_wildcard_files(client, &dir, &pattern, true).await {
                Ok(paths) => {
                    if paths.is_empty() {
                        println!("没有匹配的目录: {}", parts[2]);
                        return;
                    }
                    for remote_path in &paths {
                        let dirname = Path::new(remote_path).file_name().and_then(|n| n.to_str()).unwrap_or("download");
                        let local_dir = format!("{}/{}", local_base, dirname);
                        if let Err(e) = download_dir_cmd(client, remote_path, &local_dir, num_threads).await {
                            eprintln!("下载 {} 失败: {}", remote_path, e);
                        }
                    }
                }
                Err(e) => eprintln!("展开通配符失败: {}", e),
            }
        } else {
            let remote_dir = normalize_path(&client.session.current_remote_path, parts[2]);
            let local_dir = local_dest_for_remote(client, &remote_dir, parts.get(3).copied());
            if let Err(e) = download_dir_cmd(client, &remote_dir, &local_dir, num_threads).await {
                eprintln!("下载失败: {}", e);
            }
        }
    } else if has_wildcards(parts[1]) {
        let (dir, pattern) = parse_wildcard_path(parts[1], &client.session.current_remote_path, true);
        let local_dir = if parts.len() > 2 {
            resolve_local_path(&client.session.current_local_path, parts[2])
        } else {
            client.session.current_local_path.clone()
        };
        match expand_remote_wildcard_files(client, &dir, &pattern, false).await {
            Ok(paths) => {
                if paths.is_empty() {
                    println!("没有匹配的文件: {}", parts[1]);
                    return;
                }
                println!("匹配到 {} 个文件", paths.len());
                for remote_path in &paths {
                    let filename = Path::new(remote_path).file_name().and_then(|n| n.to_str()).unwrap_or("download");
                    let local_path = format!("{}/{}", local_dir, filename);
                    if let Err(e) = download_file_cmd(client, remote_path, &local_path, num_threads).await {
                        eprintln!("下载 {} 失败: {}", remote_path, e);
                    }
                }
            }
            Err(e) => eprintln!("展开通配符失败: {}", e),
        }
    } else {
        let remote_path = normalize_path(&client.session.current_remote_path, parts[1]);
        let local_path = local_dest_for_remote(client, &remote_path, parts.get(2).copied());
        if let Err(e) = download_file_cmd(client, &remote_path, &local_path, num_threads).await {
            eprintln!("下载失败: {}", e);
        }
    }
}

async fn cmd_mget(client: &BaiduApiClient, parts: &[&str]) {
    if parts.len() < 2 {
        eprintln!("用法: mget [-r] [-t 线程数] <远程文件> [本地路径]");
        eprintln!("支持通配符: mget *.txt  /  mget -r dir*");
        return;
    }
    let mut recursive = false;
    let mut num_threads = DEFAULT_THREADS;
    let mut args_start = 1;

    while args_start < parts.len() {
        match parts[args_start] {
            "-r" => {
                recursive = true;
                args_start += 1;
            }
            "-t" => {
                if args_start + 1 < parts.len() {
                    num_threads = parts[args_start + 1].parse().unwrap_or(DEFAULT_THREADS);
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

    // 重构 parts，变为 [cmd, ...args] 形式，用 get 统一处理
    let mut new_parts = vec!["get"];
    if recursive {
        new_parts.push("-r");
    }
    new_parts.extend_from_slice(&parts[args_start..]);
    cmd_get(client, &new_parts, recursive, Some(num_threads)).await;
}

async fn download_file_cmd(client: &BaiduApiClient, remote_path: &str, local_path: &str, num_threads: Option<usize>) -> anyhow::Result<()> {
    match num_threads {
        Some(threads) => client.download_file_mt(remote_path, local_path, threads).await,
        None => client.download_file(remote_path, local_path).await,
    }
}

async fn download_dir_cmd(client: &BaiduApiClient, remote_dir: &str, local_dir: &str, num_threads: Option<usize>) -> anyhow::Result<()> {
    match num_threads {
        Some(threads) => client.download_dir_mt(remote_dir, local_dir, threads).await,
        None => client.download_dir(remote_dir, local_dir).await,
    }
}

fn local_dest_for_remote(client: &BaiduApiClient, remote_path: &str, explicit_dest: Option<&str>) -> String {
    if let Some(dest) = explicit_dest {
        resolve_local_path(&client.session.current_local_path, dest)
    } else {
        let filename = Path::new(remote_path).file_name().and_then(|n| n.to_str()).unwrap_or("download");
        resolve_local_path(&client.session.current_local_path, filename)
    }
}

fn resolve_local_dest(client: &BaiduApiClient, src: &str, dest_arg: &str) -> String {
    if dest_arg.ends_with('/') {
        let dir = resolve_local_path(&client.session.current_local_path, dest_arg);
        let name = Path::new(src).file_name().and_then(|n| n.to_str()).unwrap_or("untitled");
        format!("{}/{}", dir, name)
    } else {
        resolve_local_path(&client.session.current_local_path, dest_arg)
    }
}

fn cmd_clear() {
    print!("\x1B[2J\x1B[1;1H");
    io::Write::flush(&mut io::stdout()).ok();
}

// ---- 确认提示 ----

fn confirm_delete(path: &str) -> bool {
    print!("确认删除 {}? (y/N): ", path);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

fn confirm_delete_batch(count: usize) -> bool {
    print!("确认删除以上 {} 个文件? (y/N): ", count);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

// ---- 配置管理 ----

fn print_config(config: &Config) {
    println!("当前配置:");
    println!("  client_id:    {}", config.client_id);
    println!("  redirect_uri: {}", config.redirect_uri);
    println!("  scope:        {}", config.scope);
    println!("  默认用户:     {}", config.default);
    println!("  用户列表:");
    for user in config.get_users() {
        let has_token = config.get_user_token(user).is_some_and(|t| !t.is_empty());
        let status = if has_token { "已授权" } else { "未授权" };
        if *user == config.default {
            println!("    * {} ({})", user, status);
        } else {
            println!("    - {} ({})", user, status);
        }
    }
}

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

/// Shell 风格的行解析：支持单引号、双引号、反斜杠转义
#[allow(clippy::while_let_on_iterator)]
fn shell_split(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            ' ' | '\t' => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '\'' => {
                while let Some(ch) = chars.next() {
                    if ch == '\'' {
                        break;
                    }
                    current.push(ch);
                }
            }
            '"' => {
                while let Some(ch) = chars.next() {
                    if ch == '"' {
                        break;
                    }
                    if ch == '\\' {
                        if let Some(next) = chars.next() {
                            current.push(next);
                        }
                    } else {
                        current.push(ch);
                    }
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}
