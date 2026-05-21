use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::constants::{OAUTH_SCOPE, PAN_BAIDU_APP_HELP_URL};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserToken {
    #[serde(default)]
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub default: String,
    pub users: HashMap<String, UserToken>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            redirect_uri: "oob".to_string(),
            scope: OAUTH_SCOPE.to_string(),
            default: "default_user".to_string(),
            users: HashMap::new(),
        }
    }
}

impl Config {
    /// 获取默认配置文件路径
    pub fn get_default_path() -> anyhow::Result<PathBuf> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| anyhow::anyhow!("无法获取用户主目录"))?;
        Ok(PathBuf::from(home).join(".bftp").join("config.json"))
    }

    /// 获取或创建用户并返回其 token（会保存配置）
    pub fn get_or_create_user_token_mut(&mut self, username: &str) -> String {
        if let Some(user) = self.users.get(username) {
            user.access_token.clone()
        } else {
            self.users.insert(
                username.to_string(),
                UserToken { access_token: String::new() },
            );
            let _ = self.save_default();
            String::new()
        }
    }

    /// 从指定路径读取配置文件，若文件不存在则创建默认配置
    pub fn load_from_path(path: &PathBuf) -> anyhow::Result<Self> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: Config = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save_to_path(path)?;
            Ok(config)
        }
    }

    /// 从默认路径读取配置文件 (~/.bftp/config.json)
    pub fn load_default() -> anyhow::Result<Self> {
        let config_path = Self::get_default_path()?;
        Self::load_from_path(&config_path)
    }

    /// 保存配置到指定路径
    pub fn save_to_path(&self, path: &PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// 保存配置到默认路径
    pub fn save_default(&self) -> anyhow::Result<()> {
        let config_path = Self::get_default_path()?;
        self.save_to_path(&config_path)
    }

    /// 获取指定用户的 access_token
    pub fn get_user_token(&self, username: &str) -> Option<&String> {
        self.users.get(username).map(|u| &u.access_token)
    }

    /// 设置指定用户的 access_token
    pub fn set_user_token(&mut self, username: &str, token: String) {
        if let Some(user) = self.users.get_mut(username) {
            user.access_token = token;
        } else {
            self.users.insert(
                username.to_string(),
                UserToken { access_token: token },
            );
        }
    }

    /// 设置默认用户
    pub fn set_default_user(&mut self, username: &str) {
        self.default = username.to_string();
    }

    /// 获取所有用户名
    pub fn get_users(&self) -> Vec<&String> {
        self.users.keys().collect()
    }

    /// 验证配置是否完整
    pub fn validate(&self) -> Result<(), String> {
        if self.client_id.is_empty() {
            return Err(format!("client_id 不能为空, 请参照 {} 文档进行应用接入", PAN_BAIDU_APP_HELP_URL));
        }
        Ok(())
    }

    /// 添加新用户
    pub fn add_user(&mut self, username: &str, token: Option<String>) {
        self.users.insert(
            username.to_string(),
            UserToken { access_token: token.unwrap_or_default() },
        );
    }

    /// 移除用户
    pub fn remove_user(&mut self, username: &str) -> Option<UserToken> {
        let result = self.users.remove(username);
        if self.default == username && !self.users.is_empty()
            && let Some(first_user) = self.users.keys().next()
        {
            self.default = first_user.clone();
        }
        result
    }

    /// 检查某个用户是否存在
    pub fn has_user(&self, username: &str) -> bool {
        self.users.contains_key(username)
    }
}
