mod config;
mod api;

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::env;

use crate::api::{BaiduApiClient, ensure_valid_token};
use crate::config::Config;


/// 处理命令行参数，返回用户名和 token
fn handle_command_line_args(mut config: Config, args: &[String]) -> (Config, String, String) {
    let username;
    let token = if args.len() > 1 {
        // 第一种方式：btfp ${user}
        username = args[1].clone();
        println!("使用指定用户: {}", username);
        config.get_or_create_user_token(&username)
    } else {
        // 第二种方式：btfp
        println!("使用默认用户: {}", config.default);
        username = config.default.clone();
        config.get_or_create_default_token()
    };

    (config, token, username)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    // 加载配置
    let config = match Config::load_default() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("加载配置失败: {}", e);
            return Ok(());
        }
    };
    // 验证配置
    if let Err(e) = config.validate() {
        eprintln!("配置验证失败: {}", e);
        return Ok(());
    }
    // 处理命令行参数，获取 token
    let (mut config, token, username) = handle_command_line_args(config, &args);
    println!("token:{}", token);
    let token = ensure_valid_token(&mut config, &username).await?;
    println!("token:{}", token);

    let client = BaiduApiClient::new(token);
    let userinfo = client.get_user_info().await?;
    println!("baidu_name: {:?} ",userinfo.baidu_name.unwrap());
    // `()` can be used when no completer is required
    let mut rl = DefaultEditor::new()?;

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                println!("Line: {}", line);
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}
