use anyhow::{Context, anyhow};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    hash::Hash,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    str, string,
};



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

    /// 预上传 - 通知网盘新建上传任务
    pub async fn precreate(&self, path: &str, size: u64, block_list: &str) -> anyhow::Result<PrecreateResponse> {
        let url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=precreate&access_token={}",
            self.access_token
        );
        let body = format!(
            "path={}&size={}&isdir=0&block_list={}&autoinit=1&rtype=1",
            urlencoding::encode(path),
            size,
            urlencoding::encode(block_list),
        );
        let response = self.client.post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send().await?;
        let result: PrecreateResponse = response.json().await?;
        if result.base.errno != 0 {
            return Err(anyhow!("预上传失败: errno={}, errmsg={:?}", result.base.errno, result.base.errmsg));
        }
        Ok(result)
    }

    /// 获取上传域名
    pub async fn locate_upload(&self, path: &str, uploadid: &str) -> anyhow::Result<String> {
        let encoded_path = urlencoding::encode(path);
        let url = format!(
            "https://d.pcs.baidu.com/rest/2.0/pcs/file?method=locateupload&appid=250528&access_token={}&path={}&uploadid={}&upload_version=2.0",
            self.access_token, encoded_path, uploadid
        );
        let response = self.client.get(&url).send().await?;
        let result: LocateUploadResponse = response.json().await?;
        if result.error_code != 0 {
            return Err(anyhow!("获取上传域名失败: error_code={}, error_msg={:?}", result.error_code, result.error_msg));
        }
        if let Some(servers) = &result.servers {
            for s in servers {
                if s.server.starts_with("https://") {
                    return Ok(s.server.clone());
                }
            }
            if let Some(first) = servers.first() {
                return Ok(first.server.clone());
            }
        }
        Err(anyhow!("未获取到可用的上传域名"))
    }

    /// 分片上传
    pub async fn upload_chunk(
        &self,
        domain: &str,
        path: &str,
        uploadid: &str,
        partseq: i32,
        data: Vec<u8>,
    ) -> anyhow::Result<String> {
        let encoded_path = urlencoding::encode(path);
        let url = format!(
            "{}/rest/2.0/pcs/superfile2?method=upload&access_token={}&type=tmpfile&path={}&uploadid={}&partseq={}",
            domain, self.access_token, encoded_path, uploadid, partseq
        );

        // 手动构造 multipart/form-data body
        let boundary = "bftp_upload_boundary";
        let mut body: Vec<u8> = Vec::new();
        write!(body, "--{}\r\n", boundary)?;
        write!(body, "Content-Disposition: form-data; name=\"file\"; filename=\"blob\"\r\n")?;
        write!(body, "Content-Type: application/octet-stream\r\n\r\n")?;
        body.extend_from_slice(&data);
        write!(body, "\r\n--{}--\r\n", boundary)?;

        let content_type = format!("multipart/form-data; boundary={}", boundary);
        let response = self.client.post(&url)
            .header("Content-Type", &content_type)
            .body(body)
            .send()
            .await?;
        let status = response.status();
        let body_text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!(
                "分片上传HTTP错误: status={}, body={}",
                status,
                body_text
            ));
        }
        let result: UploadChunkResponse = serde_json::from_str(&body_text)
            .map_err(|e| anyhow!("解析分片上传响应失败: {}, body={}", e, body_text))?;
        if let Some(errno) = result.errno {
            if errno != 0 {
                return Err(anyhow!("分片上传失败: errno={}, body={}", errno, body_text));
            }
        }
        Ok(result.md5.unwrap_or_default())
    }

    /// 创建文件 - 合并分片完成上传
    pub async fn create_file(
        &self,
        path: &str,
        size: u64,
        block_list: &str,
        uploadid: &str,
    ) -> anyhow::Result<CreateFileResponse> {
        let url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=create&access_token={}",
            self.access_token
        );
        let body = format!(
            "path={}&size={}&isdir=0&block_list={}&uploadid={}&rtype=1",
            urlencoding::encode(path),
            size,
            urlencoding::encode(block_list),
            urlencoding::encode(uploadid),
        );
        let response = self.client.post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send().await?;
        let result: CreateFileResponse = response.json().await?;
        if result.base.errno != 0 {
            return Err(anyhow!("创建文件失败: errno={}, errmsg={:?}", result.base.errno, result.base.errmsg));
        }
        Ok(result)
    }

    /// 上传单个文件到远程当前目录
    pub async fn upload_file(&self, local_path: &str, remote_filename: Option<&str>) -> anyhow::Result<()> {
        let local_file_path = Path::new(local_path);
        if !local_file_path.exists() {
            return Err(anyhow!("本地文件不存在: {}", local_path));
        }
        if !local_file_path.is_file() {
            return Err(anyhow!("不是一个文件: {}", local_path));
        }

        let filename = remote_filename.unwrap_or_else(|| {
            local_file_path.file_name().unwrap().to_str().unwrap()
        });

        let remote_path = if self.current_remote_path.ends_with('/') {
            format!("{}{}", self.current_remote_path, filename)
        } else {
            format!("{}/{}", self.current_remote_path, filename)
        };

        println!("上传文件: {} -> {}", local_path, remote_path);

        // 1. 计算 block_list
        let (file_size, block_list, chunk_count) = compute_block_list(local_path)?;
        println!("文件大小: {}, 分片数: {}", format_size(file_size), chunk_count);

        // 2. 预上传
        println!("[1/3] 预上传...");
        let precreate_result = self.precreate(&remote_path, file_size, &block_list).await?;
        let uploadid = precreate_result.uploadid.context("预上传未返回uploadid")?;
        let chunks_to_upload = precreate_result.block_list.unwrap_or_else(|| vec![0]);
        println!("uploadid: {}", uploadid);

        // 3. 获取上传域名
        println!("[2/3] 获取上传域名...");
        let domain = self.locate_upload(&remote_path, &uploadid).await?;
        println!("上传域名: {}", domain);

        // 4. 分片上传
        let total_chunks = chunks_to_upload.len();
        for (i, &chunk_idx) in chunks_to_upload.iter().enumerate() {
            let progress = format!("{}/{}", i + 1, total_chunks);
            println!("[2/3] 上传分片 {} (index={})...", progress, chunk_idx);
            let chunk_data = read_chunk(local_path, chunk_idx as usize)?;
            let chunk_md5 = self.upload_chunk(&domain, &remote_path, &uploadid, chunk_idx, chunk_data).await?;
            println!("  分片 {} md5: {}", chunk_idx, chunk_md5);
        }

        // 5. 创建文件
        println!("[3/3] 创建文件...");
        let result = self.create_file(&remote_path, file_size, &block_list, &uploadid).await?;
        println!("上传成功!");
        println!("  文件名: {}", result.server_filename.as_deref().unwrap_or(filename));
        println!("  路径: {}", result.path.as_deref().unwrap_or(&remote_path));
        println!("  大小: {}", format_size(result.size.unwrap_or(file_size)));
        println!("  fs_id: {}", result.fs_id.unwrap_or(0));

        Ok(())
    }

    /// 文件管理通用接口
    pub async fn filemanager(&self, opera: &str, filelist: &str) -> anyhow::Result<FileManagerResponse> {
        let url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=filemanager&access_token={}&opera={}&async=0",
            self.access_token, opera
        );
        let body = format!("filelist={}", urlencoding::encode(filelist));
        let response = self.client.post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await?;
        let result: FileManagerResponse = response.json().await?;
        if result.base.errno != 0 {
            return Err(anyhow!(
                "文件操作失败: errno={}, errmsg={:?}",
                result.base.errno,
                result.base.errmsg
            ));
        }
        if let Some(ref info) = result.info {
            for item in info {
                if item.errno != 0 {
                    return Err(anyhow!(
                        "文件 {} 操作失败: errno={}",
                        item.path.as_deref().unwrap_or("?"),
                        item.errno
                    ));
                }
            }
        }
        Ok(result)
    }

    /// 重命名远程文件
    pub async fn rename_file(&self, path: &str, newname: &str) -> anyhow::Result<()> {
        let filelist = serde_json::to_string(&[serde_json::json!({
            "path": path,
            "newname": newname
        })])?;
        self.filemanager("rename", &filelist).await?;
        Ok(())
    }

    /// 复制远程文件
    pub async fn copy_file(&self, path: &str, dest: &str, newname: &str) -> anyhow::Result<()> {
        let filelist = serde_json::to_string(&[serde_json::json!({
            "path": path,
            "dest": dest,
            "newname": newname
        })])?;
        self.filemanager("copy", &filelist).await?;
        Ok(())
    }

    /// 删除远程文件
    pub async fn delete_file(&self, path: &str) -> anyhow::Result<()> {
        let filelist = serde_json::to_string(&[path])?;
        self.filemanager("delete", &filelist).await?;
        Ok(())
    }

    /// 列出指定目录的文件
    pub async fn list_files_in_dir(&self, dir: &str) -> anyhow::Result<FileListResponse> {
        let url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=list&access_token={}&dir={}",
            self.access_token, dir
        );
        let response = self.client.get(&url).send().await?;
        let result: FileListResponse = response.json().await?;
        if result.base.errno != 0 {
            return Err(anyhow!("列出文件失败: errno={}", result.base.errno));
        }
        Ok(result)
    }

    /// 获取文件元信息（含下载链接 dlink）
    pub async fn get_file_metas(&self, fsids: &[u64]) -> anyhow::Result<QueryFileInfoResponse> {
        let fsids_json = serde_json::to_string(fsids)?;
        let url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/multimedia?method=filemetas&access_token={}&fsids={}&dlink=1",
            self.access_token, fsids_json
        );
        let response = self.client.get(&url).send().await?;
        let result: QueryFileInfoResponse = response.json().await?;
        if result.base.errno != 0 {
            return Err(anyhow!("获取文件元信息失败: errno={}", result.base.errno));
        }
        if result.list.is_empty() {
            return Err(anyhow!("未获取到文件元信息"));
        }
        Ok(result)
    }

    /// 递归获取目录下所有文件（支持分页）
    pub async fn recursive_list(&self, path: &str) -> anyhow::Result<Vec<FileInfo>> {
        let encoded_path = urlencoding::encode(path);
        let mut all_files: Vec<FileInfo> = Vec::new();
        let mut cursor = 0i32;

        loop {
            let url = format!(
                "https://pan.baidu.com/rest/2.0/xpan/multimedia?method=listall&access_token={}&path={}&recursion=1&start={}&limit=1000",
                self.access_token, encoded_path, cursor
            );
            let response = self.client.get(&url).send().await?;
            let result: CategoryFileListResponse = response.json().await?;
            if result.base.errno != 0 {
                return Err(anyhow!("递归列出文件失败: errno={}", result.base.errno));
            }
            if let Some(list) = result.list {
                all_files.extend(list);
            }
            if result.has_more == 0 {
                break;
            }
            cursor = result.cursor;
        }

        Ok(all_files)
    }

    /// 通过 dlink 下载文件到本地
    pub async fn download_from_url(&self, dlink: &str, local_path: &str) -> anyhow::Result<u64> {
        let url = format!("{}&access_token={}", dlink, self.access_token);
        let response = self.client.get(&url)
            .header("User-Agent", "pan.baidu.com")
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(anyhow!("下载HTTP错误: status={}, body={}", status, body));
        }
        let bytes = response.bytes().await?;
        let size = bytes.len() as u64;
        if let Some(parent) = Path::new(local_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(local_path)?;
        file.write_all(&bytes)?;
        Ok(size)
    }

    /// 下载单个远程文件（通过路径查找 fs_id → dlink → 下载）
    pub async fn download_file(&self, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
        let p = Path::new(remote_path);
        let parent_dir = p.parent().and_then(|x| x.to_str()).unwrap_or("/");
        let filename = p.file_name().and_then(|x| x.to_str()).unwrap_or("");

        // 列出父目录，查找文件 fs_id
        let file_list = self.list_files_in_dir(parent_dir).await?;
        let file_info = file_list.list.as_ref()
            .and_then(|files| files.iter().find(|f| f.server_filename == filename))
            .ok_or_else(|| anyhow!("未找到远程文件: {}", remote_path))?;

        if file_info.isdir == 1 {
            return Err(anyhow!("{} 是目录，请使用 get -r <目录> 下载", remote_path));
        }

        // 获取 dlink 并下载
        let metas = self.get_file_metas(&[file_info.fs_id]).await?;
        let file_meta = metas.list.first()
            .ok_or_else(|| anyhow!("获取下载链接失败"))?;
        let dlink = &file_meta.dlink;

        let size = self.download_from_url(dlink, local_path).await?;
        println!("下载成功: {} ({} 已写入)", format_size(file_info.size), format_size(size));
        Ok(())
    }

    /// 递归下载远程目录
    pub async fn download_dir(&self, remote_dir: &str, local_dir: &str) -> anyhow::Result<()> {
        let all_entries = self.recursive_list(remote_dir).await?;
        let files: Vec<_> = all_entries.iter().filter(|f| f.isdir == 0).collect();

        if files.is_empty() {
            println!("目录为空，没有文件可下载");
            return Ok(());
        }

        println!("共 {} 个文件待下载", files.len());

        for (i, file_info) in files.iter().enumerate() {
            // 将远程路径映射到本地路径
            let rel_path = file_info.path.strip_prefix(remote_dir)
                .unwrap_or(&file_info.path);
            let rel_path = rel_path.strip_prefix('/').unwrap_or(rel_path);
            let local_file_path = Path::new(local_dir).join(rel_path);

            println!("[{}/{}] 下载: {} -> {}",
                i + 1, files.len(),
                file_info.path,
                local_file_path.display()
            );

            let metas = self.get_file_metas(&[file_info.fs_id]).await?;
            let file_meta = metas.list.first()
                .ok_or_else(|| anyhow!("获取下载链接失败"))?;
            self.download_from_url(&file_meta.dlink, local_file_path.to_str().unwrap()).await?;
        }

        println!("下载完成: {} 个文件", files.len());
        Ok(())
    }

    /// 关键字搜索文件
    pub async fn search_files_by_keyword(
        &self,
        key: &str,
        dir: Option<&str>,
        recursion: bool,
    ) -> anyhow::Result<SearchFileByKeywordResponse> {
        let encoded_key = urlencoding::encode(key);
        let mut url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=search&access_token={}&key={}",
            self.access_token, encoded_key
        );
        if let Some(d) = dir {
            url.push_str(&format!("&dir={}", urlencoding::encode(d)));
        } else {
            url.push_str(&format!("&dir={}", urlencoding::encode(&self.current_remote_path)));
        }
        if recursion {
            url.push_str("&recursion=1");
        }
        let response = self.client.get(&url)
            .header("User-Agent", "pan.baidu.com")
            .send().await?;
        let result: SearchFileByKeywordResponse = response.json().await?;
        if result.base.errno != 0 {
            return Err(anyhow!("关键字搜索失败: errno={}, errmsg={:?}", result.base.errno, result.base.errmsg));
        }
        Ok(result)
    }

    /// 语义搜索文件
    pub async fn search_files_semantic(
        &self,
        query: &str,
        search_type: i32,
        dir: Option<&str>,
    ) -> anyhow::Result<SearchFileBySemanticResponse> {
        let dir_val = dir.unwrap_or(&self.current_remote_path);
        let url = format!(
            "https://pan.baidu.com/xpan/unisearch?access_token={}&scene=mcpserver&query={}&search_type={}&num=500&dir={}",
            self.access_token,
            urlencoding::encode(query),
            search_type,
            urlencoding::encode(dir_val),
        );
        let response = self.client.post(&url)
            .header("Content-Type", "application/json")
            .body("{}")
            .send().await?;
        let result: SearchFileBySemanticResponse = response.json().await?;
        if result.error_no != 0 {
            return Err(anyhow!("语义搜索失败: error_no={}, error_msg={:?}", result.error_no, result.error_msg));
        }
        Ok(result)
    }

    /// 获取网盘容量信息
    pub async fn get_capacity_info(&self) -> anyhow::Result<CapacityInfoResponse> {
        let url = format!(
            "https://pan.baidu.com/api/quota?access_token={}",
            self.access_token
        );
        let response = self.client.get(&url).send().await?;
        let result: CapacityInfoResponse = response.json().await?;
        if result.errno != 0 {
            return Err(anyhow!("获取容量信息失败: errno={}", result.errno));
        }
        Ok(result)
    }

    /// 创建远程目录
    pub async fn create_remote_dir(&self, path: &str) -> anyhow::Result<CreateFileResponse> {
        let url = format!(
            "https://pan.baidu.com/rest/2.0/xpan/file?method=create&access_token={}",
            self.access_token
        );
        let body = format!(
            "path={}&isdir=1&rtype=0",
            urlencoding::encode(path),
        );
        let response = self.client.post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send().await?;
        let result: CreateFileResponse = response.json().await?;
        if result.base.errno != 0 {
            return Err(anyhow!("创建远程目录失败: errno={}, errmsg={:?}", result.base.errno, result.base.errmsg));
        }
        Ok(result)
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
                    FileType::Unknown => "未知",
                })
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
            format!("{:8}", match file.category {
                FileType::Video => "视频",
                FileType::Audio => "音乐",
                FileType::Image => "图片",
                FileType::Document => "文档",
                FileType::App => "应用",
                FileType::Other => "其他",
                FileType::Torrent => "种子",
                FileType::Unknown => "未知",
            })
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
                let type_name = match file.category {
                    FileType::Video => "视频    ",
                    FileType::Audio => "音乐    ",
                    FileType::Image => "图片    ",
                    FileType::Document => "文档    ",
                    FileType::App => "应用    ",
                    FileType::Other => "其他    ",
                    FileType::Torrent => "种子    ",
                    FileType::Unknown => "未知    ",
                };
                println!("{} {:>8} {} {} [{}] {}",
                    file_type, size_str, time_str, type_name, source_name, file.path);
                if let Some(ref c) = file.content {
                    if !c.is_empty() {
                        println!("  -> {}", c);
                    }
                }
                if let Some(ref o) = file.ocr {
                    if !o.is_empty() {
                        println!("  -> OCR: {}", o);
                    }
                }
            }
        }
    } else {
        println!("(无搜索结果)");
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

// ==================== 上传辅助函数 ====================

/// 计算数据的MD5
fn compute_file_md5(data: &[u8]) -> String {
    format!("{:x}", md5::compute(data))
}

/// 计算文件的 block_list（每个4MB分片的MD5数组的JSON字符串），返回 (文件大小, block_list_json)
fn compute_block_list(file_path: &str) -> anyhow::Result<(u64, String, usize)> {
    let mut file = File::open(file_path)?;
    let file_size = file.metadata()?.len();

    const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4MB
    let mut md5s: Vec<String> = Vec::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        md5s.push(compute_file_md5(&buffer[..bytes_read]));
    }

    // 空文件发送一个空的MD5
    if md5s.is_empty() {
        md5s.push(compute_file_md5(&[]));
    }

    let block_list_str = serde_json::to_string(&md5s)?;
    Ok((file_size, block_list_str, md5s.len()))
}

/// 读取文件的指定分片（0-indexed，每片4MB）
fn read_chunk(file_path: &str, chunk_index: usize) -> anyhow::Result<Vec<u8>> {
    let mut file = File::open(file_path)?;
    const CHUNK_SIZE: u64 = 4 * 1024 * 1024;
    let offset = chunk_index as u64 * CHUNK_SIZE;
    file.seek(SeekFrom::Start(offset))?;
    let mut buffer = vec![0u8; CHUNK_SIZE as usize];
    let bytes_read = file.read(&mut buffer)?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}
