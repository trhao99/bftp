use serde::Deserialize;
use std::collections::HashMap;
use std::hash::Hash;

/// API 响应统一检查 trait
pub trait ApiResponse {
    fn is_success(&self) -> bool;
    fn error_desc(&self) -> String;
}

/// 百度网盘API响应通用结构
#[derive(Debug, Deserialize)]
pub struct BaiduApiErrNoResponse {
    pub errno: i32,
    pub errmsg: Option<String>,
}

impl ApiResponse for BaiduApiErrNoResponse {
    fn is_success(&self) -> bool {
        self.errno == 0
    }
    fn error_desc(&self) -> String {
        format!("errno={}, msg={:?}", self.errno, self.errmsg)
    }
}

/// 用户信息响应
#[derive(Debug, Deserialize)]
pub struct UserInfoResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub baidu_name: Option<String>,
    pub netdisk_name: Option<String>,
    pub avatar_url: Option<String>,
    pub vip_type: Option<i32>,
    pub uk: Option<u64>,
}

impl ApiResponse for UserInfoResponse {
    fn is_success(&self) -> bool { self.base.is_success() }
    fn error_desc(&self) -> String { self.base.error_desc() }
}

/// 容量信息
#[derive(Debug, Deserialize)]
pub struct CapacityInfoResponse {
    pub errno: i32,
    pub expire: bool,
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub request_id: Option<u64>,
}

impl ApiResponse for CapacityInfoResponse {
    fn is_success(&self) -> bool { self.errno == 0 }
    fn error_desc(&self) -> String { format!("errno={}", self.errno) }
}

/// 文件列表响应
#[derive(Debug, Deserialize)]
pub struct FileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub list: Option<Vec<FileInfo>>,
    pub guid: Option<u32>,
}

impl ApiResponse for FileListResponse {
    fn is_success(&self) -> bool { self.base.is_success() }
    fn error_desc(&self) -> String { self.base.error_desc() }
}

/// 分类文件列表（含分页）
#[derive(Debug, Deserialize)]
pub struct CategoryFileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub has_more: i32,
    pub cursor: i32,
    pub list: Option<Vec<FileInfo>>,
}

impl ApiResponse for CategoryFileListResponse {
    fn is_success(&self) -> bool { self.base.is_success() }
    fn error_desc(&self) -> String { self.base.error_desc() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde_repr::Serialize_repr, serde_repr::Deserialize_repr)]
#[repr(u32)]
pub enum FileType {
    Unknown = 0,
    Video = 1,
    Audio = 2,
    Image = 3,
    Document = 4,
    App = 5,
    Other = 6,
    Torrent = 7,
}

impl FileType {
    pub fn display(&self) -> &'static str {
        match self {
            FileType::Unknown => "未知",
            FileType::Video => "视频",
            FileType::Audio => "音乐",
            FileType::Image => "图片",
            FileType::Document => "文档",
            FileType::App => "应用",
            FileType::Other => "其他",
            FileType::Torrent => "种子",
        }
    }
}

/// 文件信息
#[derive(Debug, Deserialize)]
pub struct FileInfo {
    pub fs_id: u64,
    pub path: String,
    pub server_filename: String,
    pub size: u64,
    pub server_mtime: Option<u64>,
    pub server_ctime: Option<u64>,
    pub local_mtime: Option<u64>,
    pub local_ctime: Option<u64>,
    pub isdir: u32,
    pub category: FileType,
    pub md5: Option<String>,
    pub dir_empty: Option<i32>,
    pub thumbs: Option<HashMap<String, String>>,
}

/// 文件元信息
#[derive(Debug, Deserialize)]
pub struct FileMeta {
    pub category: FileType,
    pub dlink: String,
    pub filename: String,
    pub isdir: i32,
    pub server_ctime: i32,
    pub server_mtime: i32,
    pub size: i32,
    pub thumbs: Option<HashMap<String, String>>,
    pub height: Option<i32>,
    pub width: Option<i32>,
    pub date_taken: Option<i32>,
    pub orientation: Option<String>,
    pub media_info: Option<HashMap<String, String>>,
}

/// 下载链接响应
#[derive(Debug, Deserialize)]
pub struct QueryFileInfoResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub names: HashMap<String, String>,
    pub list: Vec<FileMeta>,
}

impl ApiResponse for QueryFileInfoResponse {
    fn is_success(&self) -> bool { self.base.is_success() }
    fn error_desc(&self) -> String { self.base.error_desc() }
}

/// 关键字搜索
#[derive(Debug, Deserialize)]
pub struct SearchFileByKeywordResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub has_more: i32,
    pub list: Vec<FileInfo>,
}

impl ApiResponse for SearchFileByKeywordResponse {
    fn is_success(&self) -> bool { self.base.is_success() }
    fn error_desc(&self) -> String { self.base.error_desc() }
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
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub pid: Option<u64>,
    #[serde(default)]
    pub ocr: Option<String>,
    pub server_ctime: u64,
    pub server_mtime: u64,
    #[serde(default)]
    pub size: Option<u64>,
}

/// 语义搜索data
#[derive(Debug, Deserialize)]
pub struct SemanticData {
    pub category: FileType,
    pub display_type: i32,
    pub list: Vec<SemanticFileInfo>,
    pub source: i32,
}

/// 语义搜索响应
#[derive(Debug, Deserialize)]
pub struct SearchFileBySemanticResponse {
    pub error_no: i32,
    pub error_msg: Option<String>,
    pub is_end: bool,
    pub request_id: u64,
    pub server_time: Option<u64>,
    pub data: Option<Vec<SemanticData>>,
}

impl ApiResponse for SearchFileBySemanticResponse {
    fn is_success(&self) -> bool { self.error_no == 0 }
    fn error_desc(&self) -> String { format!("error_no={}, msg={:?}", self.error_no, self.error_msg) }
}

/// 文件管理响应
#[derive(Debug, Deserialize)]
pub struct FileManagerResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub info: Option<Vec<FileManagerItem>>,
    pub taskid: Option<u64>,
    pub request_id: Option<u64>,
}

impl ApiResponse for FileManagerResponse {
    fn is_success(&self) -> bool { self.base.is_success() }
    fn error_desc(&self) -> String { self.base.error_desc() }
}

/// 文件管理结果项
#[derive(Debug, Deserialize)]
pub struct FileManagerItem {
    pub errno: i32,
    pub path: Option<String>,
}

/// 预上传响应
#[derive(Debug, Deserialize)]
pub struct PrecreateResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub path: Option<String>,
    pub uploadid: Option<String>,
    pub return_type: Option<i32>,
    pub block_list: Option<Vec<i32>>,
    pub request_id: Option<u64>,
}

impl ApiResponse for PrecreateResponse {
    fn is_success(&self) -> bool { self.base.is_success() }
    fn error_desc(&self) -> String { self.base.error_desc() }
}

/// 获取上传域名 - 服务器信息
#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    pub server: String,
}

/// 获取上传域名响应
#[derive(Debug, Deserialize)]
pub struct LocateUploadResponse {
    pub error_code: i32,
    pub error_msg: Option<String>,
    pub servers: Option<Vec<ServerInfo>>,
    pub request_id: Option<u64>,
}

impl ApiResponse for LocateUploadResponse {
    fn is_success(&self) -> bool { self.error_code == 0 }
    fn error_desc(&self) -> String { format!("error_code={}, msg={:?}", self.error_code, self.error_msg) }
}

/// 分片上传响应
#[derive(Debug, Deserialize)]
pub struct UploadChunkResponse {
    pub errno: Option<i32>,
    pub md5: Option<String>,
    pub request_id: Option<u64>,
}

impl ApiResponse for UploadChunkResponse {
    fn is_success(&self) -> bool {
        self.errno.is_none_or(|e| e == 0)
    }
    fn error_desc(&self) -> String {
        format!("errno={:?}", self.errno)
    }
}

/// 创建文件响应
#[derive(Debug, Deserialize)]
pub struct CreateFileResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub fs_id: Option<u64>,
    pub md5: Option<String>,
    pub server_filename: Option<String>,
    pub category: Option<i32>,
    pub path: Option<String>,
    pub size: Option<u64>,
    pub ctime: Option<u64>,
    pub mtime: Option<u64>,
    pub isdir: Option<i32>,
}

impl ApiResponse for CreateFileResponse {
    fn is_success(&self) -> bool { self.base.is_success() }
    fn error_desc(&self) -> String { self.base.error_desc() }
}
