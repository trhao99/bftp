## 连接与退出
```bash
bftp user            # 连接到制定用户，若没有则新建用户
bftp #连接到默认用户
exit 或 quit                     # 退出SFTP
bye                             # 退出SFTP
```

## 导航命令
```bash
pwd                             # 显示远程当前目录
lpwd                            # 显示本地当前目录
cd /path/to/dir                 # 切换远程目录
lcd /path/to/dir                # 切换本地目录
ls [-l]                        # 列出远程目录内容
lls [-la]                       # 列出本地目录内容
```

## 文件传输
### 上传文件（本地→远程）
```bash
put localfile                   # 上传单个文件
put -r localdir                 # 递归上传整个目录
put localfile remotefile        # 上传并重命名
```

### 下载文件（远程→本地）
```bash
get remotefile                  # 下载单个文件
get -r remotedir                # 递归下载整个目录
get remotefile localfile        # 下载并重命名
```

## 文件操作
```bash
rm filename                     # 删除远程文件
rmdir dirname                   # 删除远程空目录
mkdir dirname                   # 创建远程目录
mv oldname newname              # 重命名/移动远程文件
```