use crate::models::*;

/// 以 ls -l 风格打印文件列表
pub fn print_file_list(filesinfo: &FileListResponse) {
    if let Some(ref files) = filesinfo.list {
        for file in files {
            // 文件类型: d 表示目录, - 表示文件
            let file_type = if file.isdir == 1 { 'd' } else { '-' };

            // 文件大小（字节）
            let size_str = format_size(file.size);

            // 修改时间
            let time_str = format_timestamp(file.server_mtime.unwrap_or(0));

            // 文件类型名称（仅对非目录文件显示）
            let type_name = if file.isdir == 1 {
                String::from("dir     ")
            } else {
                format!("{:8}", file.category.display())
            };

            // 文件名
            let name = &file.server_filename;

            println!("{} {:>8} {} {} {}", file_type, size_str, time_str, type_name, name);
        }
    }
}

/// 打印关键字搜索结果
pub fn print_keyword_search_results(response: &SearchFileByKeywordResponse) {
    if response.list.is_empty() {
        println!("(无搜索结果)");
        return;
    }
    for file in &response.list {
        let file_type = if file.isdir == 1 { 'd' } else { '-' };
        let size_str = format_size(file.size);
        let time_str = format_timestamp(file.server_mtime.unwrap_or(0));
        let type_name = if file.isdir == 1 {
            String::from("dir     ")
        } else {
            format!("{:8}", file.category.display())
        };
        println!("{} {:>8} {} {} {}", file_type, size_str, time_str, type_name, file.path);
    }
    if response.has_more == 1 {
        println!("... 更多结果未显示");
    }
}

/// 打印语义搜索结果
pub fn print_semantic_search_results(response: &SearchFileBySemanticResponse) {
    if let Some(ref data_list) = response.data {
        if data_list.is_empty() {
            println!("(无搜索结果)");
            return;
        }
        for data in data_list {
            let source_name = match data.source {
                4 => "文件名",
                5 => "图片OCR",
                7 => "文档向量",
                8 => "视频向量",
                9 => "音频向量",
                11 => "文档内容",
                13 => "证件卡片",
                14 => "图片语义",
                _ => "未知来源",
            };
            for file in &data.list {
                let file_type = if file.isdir == 1 { 'd' } else { '-' };
                let size_str = format_size(file.size.unwrap_or(0));
                let time_str = format_timestamp(file.server_mtime);
                let type_name = format!("{:8}", file.category.display());
                println!("{} {:>8} {} {} [{}] {}",
                    file_type, size_str, time_str, type_name, source_name, file.path);
                if let Some(ref c) = file.content
                    && !c.is_empty()
                {
                    println!("  -> {}", c);
                }
                if let Some(ref o) = file.ocr
                    && !o.is_empty()
                {
                    println!("  -> OCR: {}", o);
                }
            }
        }
    } else {
        println!("(无搜索结果)");
    }
}

/// 格式化文件大小为可读形式
pub fn format_size(size: u64) -> String {
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

/// 格式化时间戳为可读形式 (MM-dd HH:mm)
pub fn format_timestamp(timestamp: u64) -> String {
    let secs = timestamp as i64;
    let days = secs / 86400;
    let time_in_day = secs % 86400;
    let hours = time_in_day / 3600;
    let minutes = (time_in_day % 3600) / 60;

    let (month, day) = month_day_from_days(days);
    format!("{:02}-{:02} {:02}:{:02}", month, day, hours, minutes)
}

/// Howard Hinnant's civil_from_days algorithm: days since 1970-01-01 → (month, day).
fn month_day_from_days(days: i64) -> (u32, u32) {
    // Shift epoch from 1970-01-01 to 0000-03-01
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (m, d)
}
