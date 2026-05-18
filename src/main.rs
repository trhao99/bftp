mod config;
mod api;

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::env;

use crate::api::{BaiduApiClient, ensure_valid_token, print_file_list};
use crate::config::Config;
use std::path::Path;
use std::fs;
use std::os::unix::fs::MetadataExt;


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
async fn execute_command(line: &str, client: &mut BaiduApiClient) {
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
    let (mut config, token, username) = handle_command_line_args(config, &args);
    println!("token:{}", token);
    let token = ensure_valid_token(&mut config, &username).await?;
    println!("token:{}", token);

    let mut client = BaiduApiClient::new(token);
    let userinfo = client.get_user_info().await?;
    println!("baidu_name: {:?} ",userinfo.baidu_name.unwrap());
    // `()` can be used when no completer is required
    let mut rl = DefaultEditor::new()?;

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                execute_command(&line, &mut client).await;
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
