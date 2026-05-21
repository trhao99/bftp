use rustyline::history::DefaultHistory;
use rustyline::error::ReadlineError;
use std::env;

use bftp::auth::ensure_valid_token;
use bftp::cli::{execute_command, handle_command_line_args, handle_config_command, BftpHelper};
use bftp::client::BaiduApiClient;
use bftp::config::Config;

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

    // 处理 config 子命令（不需要鉴权）
    if args.len() > 1 && args[1] == "config" {
        return handle_config_command(config, &args);
    }

    // 验证配置
    if let Err(e) = config.validate() {
        eprintln!("配置验证失败: {}", e);
        return Ok(());
    }
    // 处理命令行参数，获取 token
    let (mut config, _token, username) = handle_command_line_args(config, &args);
    let token = ensure_valid_token(&mut config, &username).await?;

    let mut client = BaiduApiClient::new(token);
    let userinfo = client.get_user_info().await?;
    println!("baidu_name: {:?} ", userinfo.baidu_name.as_deref().unwrap_or("未知"));
    // `()` can be used when no completer is required
    let mut rl = rustyline::Editor::<BftpHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(BftpHelper));

    loop {
        let prompt = format!(
            "\x1b[1;36m{}\x1b[0m:\x1b[1;32m{}\x1b[0m> ",
            username,
            client.get_current_remote_path()
        );
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                execute_command(&line, &mut client, &mut rl).await;
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
