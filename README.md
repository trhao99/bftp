# bftp

> 百度网盘 FTP 风格命令行客户端 — 在终端里像操作 FTP 一样管理你的百度网盘文件。

bftp 是一个用 Rust 编写的百度网盘交互式命令行工具，提供类似 FTP 的 shell 体验。支持文件浏览、上传、下载（多线程 + 断点续传）、搜索、语义搜索、文件移动/复制/重命名等操作。

## 功能特性

- **FTP 风格交互界面** — `cd`、`ls`、`pwd` 等熟悉的命令，支持 Tab 补全和命令历史
- **文件管理** — 支持远程和本地文件的创建、删除、重命名、移动、复制
- **文件传输** — 支持上传和下载，下载支持多线程和断点续传
- **文件搜索** — 支持关键字搜索和百度网盘语义搜索
- **多用户切换** — 支持配置多个百度账号，快速切换
- **远程 + 本地双面板** — 同时管理远程网盘和本地文件系统

## 安装

```sh
cargo build --release
```

编译产物位于 `target/release/bftp`。

## 快速开始

```sh
# 使用默认用户登录
bftp

# 指定用户登录
bftp <用户名>
```

首次使用时，程序会自动引导你完成 OAuth 授权。

## 配置管理

```bash
bftp config                       # 显示当前配置
bftp config show                  # 同上
bftp config list-users            # 列出所有用户
bftp config add-user <用户名>      # 添加新用户
bftp config remove-user <用户名>   # 删除用户
bftp config set-default <用户名>   # 切换默认用户
```

## 导航命令

```bash
pwd                               # 显示远程当前目录
lpwd                              # 显示本地当前目录
cd /path/to/dir                   # 切换远程目录
lcd /path/to/dir                  # 切换本地目录
ls                                # 列出远程目录内容
lls [-la]                         # 列出本地目录内容
mkdir dirname                     # 创建远程目录
lmkdir dirname                    # 创建本地目录
quota                             # 显示网盘容量信息
clear                             # 清空控制台
exit / quit / bye                 # 退出程序
```

## 文件传输

### 上传文件（本地 → 远程）

```bash
put localfile                     # 上传单个文件到远程当前目录
put localfile remotefile          # 上传并重命名
```

### 下载文件（远程 → 本地）

```bash
get remotefile                    # 下载单个文件到本地当前目录
get remotefile localfile          # 下载并指定本地路径
get -r remotedir                  # 递归下载整个目录到本地当前目录
get -r remotedir localdir         # 递归下载到指定本地目录
```

### 多线程下载

```bash
mget remotefile [localfile]       # 多线程下载（默认 4 线程）
mget -t N remotefile [localfile]  # 指定线程数下载
mget -r remotedir [localdir]      # 多线程递归下载目录
```

## 远程文件操作

```bash
rename file newname               # 重命名远程文件
mv source dest                    # 移动远程文件（同目录→重命名，跨目录→复制后删除）
cp source dest                    # 复制远程文件到目标路径
rm filename                       # 删除远程文件
```

## 远程文件搜索

```bash
search keyword [-r] [dir]         # 关键字搜索远程文件，-r 递归搜索子目录
semsearch query [-t 0|1|2] [dir]  # 语义搜索远程文件
                                  #   -t 0: 关键字搜索
                                  #   -t 1: 语义搜索（默认）
                                  #   -t 2: 自动（查询 >5 字符使用语义）
```

## 本地文件操作

```bash
lmv source dest                   # 移动本地文件
lcp source dest                   # 复制本地文件
lrm filename                      # 删除本地文件
```

## 路径说明

- 远程路径支持绝对路径（以 `/` 开头）和相对路径（基于远程当前目录）
- 本地路径支持绝对路径和相对路径（基于本地当前目录）
- `cp`、`mv`、`lcp`、`lmv` 的目标路径以 `/` 结尾时视为目录，保留源文件名

## TODO

1. [x] 多线程下载
2. 命令支持通配符
