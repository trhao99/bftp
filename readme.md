## 连接与退出
```bash
bftp user                        # 连接到指定用户，若没有则新建用户
bftp                             # 连接到默认用户
exit / quit / bye                # 退出
```

## 导航命令
```bash
pwd                              # 显示远程当前目录
lpwd                             # 显示本地当前目录
cd /path/to/dir                  # 切换远程目录
lcd /path/to/dir                 # 切换本地目录
ls                               # 列出远程目录内容
lls [-la]                        # 列出本地目录内容
mkdir dirname                    # 创建远程目录
lmkdir dirname                   # 创建本地目录
quota                            # 显示网盘容量信息
clear                            # 清空控制台
```

## 文件传输
### 上传文件（本地→远程）
```bash
put localfile                    # 上传单个文件到远程当前目录
put localfile remotefile         # 上传并重命名
```

### 下载文件（远程→本地）
```bash
get remotefile                   # 下载单个文件到本地当前目录
get remotefile localfile         # 下载并重命名/指定本地路径
get -r remotedir                 # 递归下载整个目录到本地当前目录
get -r remotedir localdir        # 递归下载到指定本地目录
```

## 远程文件操作
```bash
rename file newname              # 重命名远程文件（同目录内）
mv source dest                   # 移动远程文件（同目录→重命名，跨目录→复制后删除）
cp source dest                   # 复制远程文件到目标路径
rm filename                      # 删除远程文件
```

## 远程文件搜索
```bash
search keyword [-r] [dir]        # 关键字搜索远程文件，-r 递归搜索子目录
semsearch query [-t 0|1|2] [dir] # 语义搜索远程文件
                                 #   -t 0: 关键字搜索
                                 #   -t 1: 语义搜索（默认）
                                 #   -t 2: 自动（查询>5字符使用语义）
```

## 本地文件操作
```bash
lmv source dest                  # 移动本地文件
lcp source dest                  # 复制本地文件
lrm filename                     # 删除本地文件
```

## 路径说明
- 远程路径支持绝对路径（以 `/` 开头）和相对路径（基于远程当前目录）
- 本地路径支持绝对路径和相对路径（基于本地当前目录）
- `cp`、`mv`、`lcp`、`lmv` 的目标路径以 `/` 结尾时视为目录，保留源文件名
