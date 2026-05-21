use std::path::Path;

/// 规范化路径（支持远程和本地路径）
pub fn normalize_path(current: &str, target: &str) -> String {
    if target.starts_with('/') {
        simplify_path(target)
    } else if target == ".." {
        let parent = if current == "/" {
            "/"
        } else {
            let trimmed = current.trim_end_matches('/');
            match trimmed.rfind('/') {
                Some(pos) => {
                    if pos == 0 { "/" } else { &trimmed[..pos] }
                }
                None => "/",
            }
        };
        parent.to_string()
    } else if target == "." {
        current.to_string()
    } else {
        let new_path = if current.ends_with('/') {
            format!("{}{}", current, target)
        } else {
            format!("{}/{}", current, target)
        };
        simplify_path(&new_path)
    }
}

/// 简化路径（去除多余的 / 和 ..）
pub fn simplify_path(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => continue,
            ".." => { stack.pop(); }
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
pub fn parse_dest_path(current: &str, dest: &str, src: &str) -> (String, String) {
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

/// 解析本地文件路径（支持相对路径和绝对路径）
pub fn resolve_local_path(current_local_path: &str, target: &str) -> String {
    if target.starts_with('/') {
        target.to_string()
    } else if current_local_path.ends_with('/') {
        format!("{}{}", current_local_path, target)
    } else {
        format!("{}/{}", current_local_path, target)
    }
}
