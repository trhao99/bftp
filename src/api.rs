use anyhow::{anyhow, Context};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// 百度网盘API响应通用结构
#[derive(Debug, Deserialize)]
pub struct BaiduApiErrNoResponse {
    // 表示具体错误码
    pub errno: i32,
    // 有关该错误的描述
    pub errmsg: Option<String>,
    // 发起请求的请求 Id
    pub request_id: Option<String>,
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

/// 文件列表响应
#[derive(Debug, Deserialize)]
pub struct FileListResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub list: Option<Vec<FileInfo>>,
    pub cursor: Option<String>,
    pub has_more: Option<bool>,
}

/// 文件信息
#[derive(Debug, Deserialize)]
pub struct FileInfo {
    pub fs_id: u64,
    pub path: String,
    pub size: u64,
    pub isdir: u32,
    pub filename: String,
    #[serde(rename = "server_filename")]
    pub server_filename: String,
    pub mtime: u64,
    pub ctime: u64,
}

/// 下载链接响应
#[derive(Debug, Deserialize)]
pub struct DownloadUrlResponse {
    #[serde(flatten)]
    pub base: BaiduApiErrNoResponse,
    pub dlink: Option<String>,
}

/// 百度网盘API客户端
pub struct BaiduApiClient {
    client: Client,
    access_token: String,
}

impl BaiduApiClient {
    /// 创建新的API客户端
    pub fn new(access_token: String) -> Self {
        Self {
            client: Client::new(),
            access_token,
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

    /// 验证token是否有效
    pub async fn verify_token(&self) -> bool {
        match self.get_user_info().await {
            Ok(info) => info.base.errno == 0,
            Err(_) => false,
        }
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
        Err(anyhow!("未找到access_token参数，请确保粘贴的是完整的回调地址"))
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
