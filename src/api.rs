use anyhow::{Context, anyhow};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    hash::Hash,
    io::{self, Write},
    str, string,
};
use serde_json::Value;


/// 百度网盘API响应通用结构
#[derive(Debug, Deserialize)]
pub struct BaiduApiErrNoResponse {
    // 表示具体错误码
    pub errno: i32,
    // 有关该错误的描述
    pub errmsg: Option<String>,
    // 发起请求的请求 Id
    // pub request_id: u64,
}

/// 用户信息响应
#[derive(Debug, Deserialize)]
pub struct UserInfoResponse {
    // 公共错误码
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    // 百度账号
    pub baidu_name: Option<String>,
    // 网盘账号
    pub netdisk_name: Option<String>,
    // 头像地址
    pub avatar_url: Option<String>,
    // 会员类型
    pub vip_type: Option<i32>,
    // 用户ID
    pub uk: Option<u64>,
}
/// 容量信息
#[derive(Debug, Deserialize)]
pub struct CapacityInfoResponse {
    // 总空间大小 单位B
    pub total: u32,
    // 7天内是否有容量到期
    pub expire: bool,
    // 已使用大小 单位B
    pub used: u32,
    // 免费容量 单位B
    pub free: u32,
}

/// 文件列表响应
#[derive(Debug, Deserialize)]
pub struct FileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub list: Option<Vec<FileInfo>>,
    pub guid: Option<u32>,
}
/// 递归文件列表响应
#[derive(Debug, Deserialize)]
pub struct RFileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub has_more: i32,
    pub cursor: i32,
    pub list: Option<Vec<FileInfo>>,
}
/// 文档列表响应
#[derive(Debug, Deserialize)]
pub struct DocFileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub guid: Option<u32>,
    pub guid_info: Option<String>,
    pub list: Option<Vec<FileInfo>>,
}
/// 获取图片列表
#[derive(Debug, Deserialize)]
pub struct PicFileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub guid: Option<u32>,
    pub guid_info: Option<String>,
    pub info: Option<Vec<FileInfo>>,
}
/// 获取视频列表
#[derive(Debug, Deserialize)]
pub struct VideoFileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub guid: Option<u32>,
    pub guid_info: Option<String>,
    pub info: Option<Vec<FileInfo>>,
}
/// 获取bt列表
#[derive(Debug, Deserialize)]
pub struct BtFileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub guid: Option<u32>,
    pub guid_info: Option<String>,
    pub info: Option<Vec<FileInfo>>,
}
/// 分类文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    pub total: u64,
    pub size: u64,
    pub count: u64,
}
/// 分类文件总个数信息
#[derive(Debug, Deserialize)]
pub struct CategoryInfoResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub info: HashMap<String, CategoryStats>,
}
/// 分类文件列表
#[derive(Debug, Deserialize)]
pub struct CategoryFileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    // 是否还有数据，0没有，1有。如果has_more=1，list为空，尝试去除ext筛选参数，或取cursor的值作为start参数进行第二次请求
    pub has_more: i32,
    // 下一次查询的起点
    pub cursor: i32,
    pub list: Option<Vec<FileInfo>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde_repr::Serialize_repr, serde_repr::Deserialize_repr)]
#[repr(u32)]
pub enum FileType {
    Video = 1,    // 视频
    Audio = 2,    // 音频
    Image = 3,    // 图片
    Document = 4, // 文档
    App = 5,      // 应用
    Other = 6,    // 其他
    Torrent = 7,  // 种子
}

/// 文件信息
#[derive(Debug, Deserialize)]
pub struct FileInfo {
    // 文件在云端的唯一标识ID
    pub fs_id: u64,
    // 文件的绝对路径
    pub path: String,
    // 文件名称
    pub server_filename: String,
    // 文件大小，单位B
    pub size: u64,
    // 文件在服务器修改时间
    pub server_mtime: Option<u64>,
    // 文件在服务器创建时间
    pub server_ctime: Option<u64>,
    // 文件在客户端修改时间
    pub local_mtime: Option<u64>,
    // 文件在客户端创建时间
    pub local_ctime: Option<u64>,
    // 是否为目录，0 文件、1 目录
    pub isdir: u32,
    // 文件类型，1 视频、2 音频、3 图片、4 文档、5 应用、6 其他、7 种子
    pub category: FileType,
    // 云端哈希（非文件真实MD5），只有是文件类型时，该字段才存在
    pub md5: Option<String>,
    // 该目录是否存在子目录，只有请求参数web=1且该条目为目录时，该字段才存在， 0为存在， 1为不存在
    pub dir_empty: Option<i32>,
    // 只有请求参数web=1且该条目分类为图片时，该字段才存在，包含三个尺寸的缩略图URL；不传web参数，则不返回缩略图地址
    pub thumbs: Option<HashMap<String, String>>,
}
/// 文件元信息
#[derive(Debug, Deserialize)]
pub struct FileMeta {
    // 文件类型，含义如下：1 视频， 2 音乐，3 图片，4 文档，5 应用，6 其他，7 种子
    pub category: FileType,
    // 文件下载地址，参考下载文档进行下载操作。注意unicode解码处理。
    pub dlink: String,
    // 文件名
    pub filename: String,
    // 是否是目录，为1表示目录，为0表示非目录
    pub isdir: i32,
    // 文件的服务器创建Unix时间戳，单位秒
    pub server_ctime: i32,
    // 文件的服务器修改Unix时间戳，单位秒
    pub server_mtime: i32,
    // 文件大小，单位字节
    pub size: i32,
    // 缩略图地址，包含四种分辨率。详细尺寸参考响应示例
    pub thumbs: HashMap<String, String>,
    // 图片高度
    pub height: i32,
    // 图片宽度
    pub width: i32,
    // 图片拍摄时间
    pub date_taken: i32,
    // 图片旋转方向信息
    pub orientation: String,
    // 视频信息。
    pub media_info: HashMap<String, String>,
}
/// 下载链接响应
#[derive(Debug, Deserialize)]
pub struct QueryFileInfoResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    // 如果查询共享目录，该字段为共享目录文件上传者的uk和账户名称
    pub names: HashMap<String, String>,
    // 文件信息列表
    pub list: Vec<FileMeta>,
}
/// 关键字搜索
#[derive(Debug, Deserialize)]
pub struct SearchFileByKeywordResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    // 是否还有下一页
    pub has_more: i32,
    // 文件信息列表
    pub list: Vec<FileInfo>,
}
/// 语义搜索文件信息
#[derive(Debug, Deserialize)]
pub struct SemanticFileInfo {
    pub category: FileType,
    pub filename: String,
    pub fsid: u64,
    pub isdir: i32,
    pub parent_path: String,
    pub path: String,
    pub content: String,
    pub pid: u64,
    pub ocr: String,
    pub server_ctime: u64,
    pub server_mtime: u64,
    pub size: u64
}
/// 语义搜索data
#[derive(Debug, Deserialize)]
pub struct SemanticData {
    pub category: FileType,
    pub display_type: i32,
    pub list: Vec<SemanticFileInfo>,
    pub source: i32
}
/// 语义搜索
#[derive(Debug, Deserialize)]
pub struct SearchFileBySemanticResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    // 文件信息列表
    pub data: Vec<SemanticData>,
}
/// 管理文件响应
#[derive(Debug, Deserialize)]
pub struct ManageFileResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
}
/// 百度网盘API客户端
pub struct BaiduApiClient {
    client: Client,
    access_token: String,
    current_remote_path: String,
    current_local_path: String,
}

impl BaiduApiClient {
    /// 创建新的API客户端
    pub fn new(access_token: String) -> Self {
        Self {
            client: Client::new(),
            access_token,
            current_local_path: String::from("/"),
            current_remote_path: String::from("/")
        }
    }
    /// 获取用户信息（验证token）
    pub async fn get_user_info(&self) -> anyhow::Result<UserInfoResponse> {
        let url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/nas?method=uinfo&access_token={}&vip_version=v2",
            self.access_token
        );

        let response = self.client.get(&url).send().await?;
        let user_info: UserInfoResponse = response.json().await?;

        Ok(user_info)
    }
    pub async fn get_remote_current_path_files_info(&self) -> anyhow::Result<FileListResponse> {
        let url: String = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=list&access_token={}&dir={}",
            self.access_token,
            self.current_remote_path
        );
        let response = self.client.get(&url).send().await?;
        // let text = response.text().await?;
        // println!("响应体： {}", text);
        // let files_info: FileListResponse = serde_json::from_str(&text)?;
        let files_info: FileListResponse = response.json().await?;
        Ok(files_info)
    }

    /// 验证token是否有效
    pub async fn verify_token(&self) -> bool {
        if let Ok(info) = self.get_user_info().await {
            info.base.errno == 0
        } else {
            false
        }
    }

    /// 获取当前远程路径
    pub fn get_current_remote_path(&self) -> &str {
        &self.current_remote_path
    }

    /// 获取当前本地路径
    pub fn get_current_local_path(&self) -> &str {
        &self.current_local_path
    }

    /// 设置当前远程路径
    pub fn set_current_remote_path(&mut self, path: String) {
        self.current_remote_path = path;
    }

    /// 设置当前本地路径
    pub fn set_current_local_path(&mut self, path: String) {
        self.current_local_path = path;
    }
}

/// 从回调URL片段中提取access_token
fn extract_access_token(callback_url: &str) -> anyhow::Result<String> {
    // 正则表达式匹配 access_token=xxx
    let re = Regex::new(r"access_token=([^&]+)")?;

    if let Some(caps) = re.captures(callback_url) {
        let token = caps[1].to_string();
        if token.is_empty() {
            return Err(anyhow!("提取到的access_token为空"));
        }
        println!("\n✓ 成功获取access_token");
        Ok(token)
    } else {
        Err(anyhow!(
            "未找到access_token参数，请确保粘贴的是完整的回调地址"
        ))
    }
}

/// 简化模式授权流程
pub async fn start_implicit_grant_flow(
    client_id: &str,
    redirect_uri: &str,
) -> anyhow::Result<String> {
    println!("\n========== 百度网盘授权流程 ==========");
    println!("需要您的授权才能访问网盘内容。");

    // 构建授权URL
    let auth_url = format!(
        "https://openapi.baidu.com/oauth/2.0/authorize?response_type=token&client_id={}&redirect_uri={}&scope=basic,netdisk",
        client_id, redirect_uri
    );

    println!("\n1. 请在浏览器中打开以下URL：");
    println!("{}\n", auth_url);

    println!("2. 登录您的百度账号并同意授权");
    println!("3. 授权成功后，页面会跳转到一个地址");
    println!("4. 请将跳转后的完整地址（包含#后面的部分）粘贴到这里：");

    // 获取用户输入的回调URL片段
    let mut callback_fragment = String::new();
    io::stdout().flush()?;
    io::stdin().read_line(&mut callback_fragment)?;
    let callback_fragment = callback_fragment.trim();

    // 提取access_token
    extract_access_token(callback_fragment)
        .context("无法从回调地址中提取access_token，请确保粘贴完整的地址")
}

/// 完整的token验证和刷新流程
pub async fn ensure_valid_token(
    config: &mut crate::config::Config,
    username: &str,
) -> anyhow::Result<String> {
    // 获取当前token
    let current_token = {
        if let Some(token) = config.get_user_token(username) {
            token.clone()
        } else {
            String::new()
        }
    };

    // 验证token
    if !current_token.is_empty() {
        println!("验证现有token...");
        let client = BaiduApiClient::new(current_token.clone());

        if client.verify_token().await {
            println!("✓ token有效");
            return Ok(current_token);
        } else {
            println!("✗ token无效或已过期");
        }
    } else {
        println!("未找到token，需要获取新token");
    }

    // 重新获取token
    println!("\n开始重新获取token...");
    let new_token = start_implicit_grant_flow(&config.client_id, &config.redirect_uri).await?;

    // 保存新token
    config.set_user_token(username, new_token.clone());
    config.save_default()?;

    println!("✓ 新token已保存");
    Ok(new_token)
}
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
                format!("{:8}", match file.category {
                    FileType::Video => "视频",
                    FileType::Audio => "音乐",
                    FileType::Image => "图片",
                    FileType::Document => "文档",
                    FileType::App => "应用",
                    FileType::Other => "其他",
                    FileType::Torrent => "种子",
                })
            };

            // 文件名
            let name = &file.server_filename;

            println!("{} {:>8} {} {} {}", file_type, size_str, time_str, type_name, name);
        }
    }
}

/// 格式化文件大小为可读形式
fn format_size(size: u64) -> String {
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
fn format_timestamp(timestamp: u64) -> String {
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
