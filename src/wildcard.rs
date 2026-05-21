use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{LazyLock, RwLock};

use crate::client::BaiduApiClient;
use crate::path_utils::{normalize_path, resolve_local_path, simplify_path};

/// 缓存已编译的 glob 正则表达式
static REGEX_CACHE: LazyLock<RwLock<HashMap<String, regex::Regex>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 检查路径是否包含通配符
pub fn has_wildcards(path: &str) -> bool {
    path.contains('*') || path.contains('?') || path.contains('[')
}

/// 将 glob 通配符模式转换为正则表达式
fn glob_to_regex(pattern: &str) -> String {
    let mut re = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '[' => {
                re.push('[');
                i += 1;
                if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
                    re.push('^');
                    i += 1;
                }
                while i < chars.len() && chars[i] != ']' {
                    re.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    re.push(']');
                }
            }
            '.' | '+' | '(' | ')' | '^' | '$' | '{' | '}' | '|' | '\\' => {
                re.push('\\');
                re.push(chars[i]);
            }
            c => re.push(c),
        }
        i += 1;
    }
    re.push('$');
    re
}

/// 用 glob 模式匹配文件名（带缓存）
pub fn glob_match(pattern: &str, name: &str) -> bool {
    // 先读缓存
    {
        let cache = REGEX_CACHE.read().unwrap();
        if let Some(re) = cache.get(pattern) {
            return re.is_match(name);
        }
    }
    // 编译并插入缓存
    let re_str = glob_to_regex(pattern);
    if let Ok(re) = regex::Regex::new(&re_str) {
        let mut cache = REGEX_CACHE.write().unwrap();
        cache.insert(pattern.to_string(), re.clone());
        re.is_match(name)
    } else {
        false
    }
}

/// 解析包含通配符的路径，返回 (目录, 模式)
pub fn parse_wildcard_path(path: &str, current_dir: &str, is_remote: bool) -> (String, String) {
    let first_wc = path.find(['*', '?', '[']).unwrap();
    let dir_end = path[..first_wc].rfind('/').map(|i| i + 1).unwrap_or(0);
    let dir_part = &path[..dir_end];
    let pattern = &path[dir_end..];

    let resolved_dir = if dir_part.is_empty() || dir_part == "." {
        current_dir.to_string()
    } else if dir_part.starts_with('/') {
        if is_remote { simplify_path(dir_part) } else { dir_part.to_string() }
    } else if is_remote {
        normalize_path(current_dir, dir_part)
    } else {
        resolve_local_path(current_dir, dir_part)
    };

    (resolved_dir, pattern.to_string())
}

/// 展开本地通配符，返回匹配的文件完整路径列表
pub fn expand_local_wildcard_files(dir: &str, pattern: &str) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if glob_match(pattern, &name) {
            files.push(entry.path().to_string_lossy().to_string());
        }
    }
    Ok(files)
}

/// 展开本地通配符，返回匹配的所有条目（文件 + 目录）路径列表
pub fn expand_local_wildcard_all(dir: &str, pattern: &str) -> anyhow::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if glob_match(pattern, &name) {
            entries.push(entry.path().to_string_lossy().to_string());
        }
    }
    Ok(entries)
}

/// 删除本地文件或目录
pub fn remove_local_path(path: &str) -> anyhow::Result<()> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(anyhow::anyhow!("路径不存在: {}", path));
    }
    if p.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// 展开远程通配符，返回匹配的文件路径列表
pub async fn expand_remote_wildcard_files(
    client: &BaiduApiClient,
    dir: &str,
    pattern: &str,
    dirs_only: bool,
) -> anyhow::Result<Vec<String>> {
    let file_list = client.list_files_in_dir(dir).await?;
    let paths: Vec<String> = file_list
        .list
        .iter()
        .flat_map(|l| l.iter())
        .filter(|f| {
            let type_ok = if dirs_only { f.isdir == 1 } else { f.isdir == 0 };
            type_ok && glob_match(pattern, &f.server_filename)
        })
        .map(|f| f.path.clone())
        .collect();
    Ok(paths)
}
