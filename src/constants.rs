/// 百度网盘 API 基础 URL
pub const PAN_BAIDU_API_BASE: &str = "https://pan.baidu.com/rest/2.0";

/// 百度 PCS 上传服务基础 URL
pub const D_PCS_BAIDU_BASE: &str = "https://d.pcs.baidu.com/rest/2.0/pcs";

/// PCS 上传服务路径前缀
pub const PCS_UPLOAD_PATH: &str = "/rest/2.0/pcs/superfile2";

/// 百度网盘开放平台 OAuth 授权地址
pub const OAUTH_AUTHORIZE_URL: &str = "https://openapi.baidu.com/oauth/2.0/authorize";

/// 百度网盘 App ID（网盘官方应用）
pub const APP_ID: &str = "250528";

/// 上传版本
pub const UPLOAD_VERSION: &str = "2.0";

/// OAuth Scope
pub const OAUTH_SCOPE: &str = "basic,netdisk";

/// 文件分片大小：4MB
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// 分片大小（u64 版本）
pub const CHUNK_SIZE_U64: u64 = CHUNK_SIZE as u64;

/// 多线程下载最低阈值：小于此大小的文件使用单线程
pub const MULTITHREAD_MIN_SIZE: u64 = 4 * 1024 * 1024;

/// 默认下载线程数
pub const DEFAULT_THREADS: usize = 4;

/// HTTP User-Agent
pub const USER_AGENT: &str = "pan.baidu.com";
