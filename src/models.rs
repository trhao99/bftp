#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;

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
    pub errno: i32,
    // 总空间大小 单位B
    pub total: u64,
    // 7天内是否有容量到期
    pub expire: bool,
    // 已使用大小 单位B
    pub used: u64,
    // 免费容量 单位B
    pub free: u64,
    pub request_id: Option<u64>,
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
    Unknown = 0,  // 未知
    Video = 1,    // 视频
    Audio = 2,    // 音频
    Image = 3,    // 图片
    Document = 4, // 文档
    App = 5,      // 应用
    Other = 6,    // 其他
    Torrent = 7,  // 种子
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
    pub thumbs: Option<HashMap<String, String>>,
    // 图片高度
    pub height: Option<i32>,
    // 图片宽度
    pub width: Option<i32>,
    // 图片拍摄时间
    pub date_taken: Option<i32>,
    // 图片旋转方向信息
    pub orientation: Option<String>,
    // 视频信息。
    pub media_info: Option<HashMap<String, String>>,
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
    pub source: i32
}
/// 语义搜索响应
#[derive(Debug, Deserialize)]
pub struct SearchFileBySemanticResponse {
    pub error_no: i32,
    pub error_msg: Option<String>,
    pub is_end: bool,
    pub request_id: u64,
    pub server_time: Option<u64>,
    // 文件信息列表
    pub data: Option<Vec<SemanticData>>,
}
/// 管理文件响应
#[derive(Debug, Deserialize)]
pub struct ManageFileResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
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

/// 分片上传响应
#[derive(Debug, Deserialize)]
pub struct UploadChunkResponse {
    pub errno: Option<i32>,
    pub md5: Option<String>,
    pub request_id: Option<u64>,
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
