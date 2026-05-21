/// 会话状态：持有认证 token 和当前工作路径
#[derive(Debug, Clone)]
pub struct Session {
    pub access_token: String,
    pub current_remote_path: String,
    pub current_local_path: String,
}

impl Session {
    pub fn new(access_token: String) -> Self {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| String::from("/"));
        Self {
            access_token,
            current_remote_path: String::from("/"),
            current_local_path: home,
        }
    }
}
