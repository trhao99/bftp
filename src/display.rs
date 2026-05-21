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
    let days_since_epoch = secs / 86400;
    let time_in_day = secs % 86400;

    let hours = time_in_day / 3600;
    let minutes = (time_in_day % 3600) / 60;

    // 粗略计算月份和日期（从1970-01-01开始）
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
