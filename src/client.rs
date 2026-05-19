use anyhow::{Context, anyhow};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::display::format_size;
use crate::models::*;

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
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| String::from("/"));
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            access_token,
            current_local_path: home,
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

    /// 通过 dlink 下载文件到本地，resume 为已下载字节数（0 表示全新下载）
    pub async fn download_from_url(&self, dlink: &str, local_path: &str, file_size: u64, resume: u64) -> anyhow::Result<u64> {
        let url = format!("{}&access_token={}", dlink, self.access_token);
        let mut req = self.client.get(&url)
            .header("User-Agent", "pan.baidu.com");
        if resume > 0 {
            req = req.header("Range", format!("bytes={}-", resume));
        }

        let mut response = req.send().await?;
        let status = response.status();
        if !status.is_success() && status.as_u16() != 206 {
            let body = response.text().await?;
            return Err(anyhow!("下载HTTP错误: status={}, body={}", status, body));
        }

        if let Some(parent) = Path::new(local_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let pb = ProgressBar::new(file_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{wide_bar} {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                .unwrap()
        );
        pb.set_message(local_path.to_string());
        if resume > 0 {
            pb.set_position(resume);
        }

        let mut file = if resume > 0 {
            OpenOptions::new().write(true).open(local_path)?
        } else {
            File::create(local_path)?
        };
        let mut downloaded: u64 = resume;
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }
        pb.finish_and_clear();

        Ok(downloaded - resume)
    }

    /// 通过 dlink 多线程下载文件，resume 为已下载字节数
    pub async fn download_from_url_multithreaded(
        &self,
        dlink: &str,
        local_path: &str,
        file_size: u64,
        num_threads: usize,
        resume: u64,
    ) -> anyhow::Result<u64> {
        if num_threads <= 1 || file_size.saturating_sub(resume) < 4 * 1024 * 1024 {
            return self.download_from_url(dlink, local_path, file_size, resume).await;
        }

        let url = format!("{}&access_token={}", dlink, self.access_token);

        if let Some(parent) = Path::new(local_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        if resume == 0 {
            let file = File::create(local_path)?;
            file.set_len(file_size)?;
        } else {
            let file = OpenOptions::new().write(true).open(local_path)?;
            file.set_len(file_size)?;
        };

        let progress_path = format!("{}.bftp_part", local_path);
        let total = Arc::new(AtomicU64::new(resume));
        File::create(&progress_path)?.write_at(&resume.to_le_bytes(), 0)?;

        let pb = Arc::new(ProgressBar::new(file_size));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{wide_bar} {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                .unwrap()
        );
        pb.set_message(local_path.to_string());
        if resume > 0 {
            pb.set_position(resume);
        }

        let remaining = file_size - resume;
        let chunk_size = remaining.div_ceil(num_threads as u64);
        let mut handles = Vec::with_capacity(num_threads);

        for i in 0..num_threads {
            let start = resume + i as u64 * chunk_size;
            if start >= file_size {
                break;
            }
            let end = std::cmp::min(start + chunk_size, file_size) - 1;
            let url = url.clone();
            let client = self.client.clone();
            let local_path = local_path.to_string();
            let pb = pb.clone();
            let total = total.clone();
            let progress_path = progress_path.clone();

            handles.push(tokio::spawn(async move {
                let mut response = client
                    .get(&url)
                    .header("User-Agent", "pan.baidu.com")
                    .header("Range", format!("bytes={}-{}", start, end))
                    .send()
                    .await
                    .context("分片请求发送失败")?;

                let status = response.status();
                if !status.is_success() && status.as_u16() != 206 {
                    let body = response.text().await.unwrap_or_default();
                    return Err(anyhow!("下载分片 HTTP 错误: status={}, body={}", status, body));
                }

                let file = OpenOptions::new().write(true).open(&local_path)?;
                let mut offset = start;
                while let Some(chunk) = response.chunk().await? {
                    file.write_at(&chunk, offset)?;
                    offset += chunk.len() as u64;
                    pb.inc(chunk.len() as u64);
                    let n = total.fetch_add(chunk.len() as u64, Ordering::Relaxed) + chunk.len() as u64;
                    let pf = OpenOptions::new().write(true).open(&progress_path)?;
                    pf.write_at(&n.to_le_bytes(), 0)?;
                }

                Ok::<_, anyhow::Error>(offset - start)
            }));
        }

        let mut downloaded: u64 = 0;
        for handle in handles {
            match handle.await.context("分片任务失败")?? {
                size => downloaded += size,
            }
        }
        pb.finish_and_clear();
        std::fs::remove_file(&progress_path).ok();

        Ok(downloaded)
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

        let resume = check_resume(local_path, file_info.size);
        if resume == file_info.size {
            println!("文件已存在，跳过: {}", local_path);
            return Ok(());
        }
        let new_bytes = self.download_from_url(dlink, local_path, file_info.size, resume).await?;
        if resume > 0 {
            println!("下载完成: {} (续传 {})", format_size(resume + new_bytes), format_size(resume));
        } else {
            println!("下载成功: {} ({} 已写入)", format_size(file_info.size), format_size(new_bytes));
        }
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
            let lp = local_file_path.to_str().unwrap();
            let resume = check_resume(lp, file_info.size);
            if resume == file_info.size {
                println!("[{}/{}] 文件已存在，跳过: {}", i + 1, files.len(), lp);
                continue;
            }
            self.download_from_url(&file_meta.dlink, lp, file_info.size, resume).await?;
        }

        println!("下载完成: {} 个文件", files.len());
        Ok(())
    }

    /// 多线程下载单个远程文件
    pub async fn download_file_mt(
        &self,
        remote_path: &str,
        local_path: &str,
        num_threads: usize,
    ) -> anyhow::Result<()> {
        let p = Path::new(remote_path);
        let parent_dir = p.parent().and_then(|x| x.to_str()).unwrap_or("/");
        let filename = p.file_name().and_then(|x| x.to_str()).unwrap_or("");

        let file_list = self.list_files_in_dir(parent_dir).await?;
        let file_info = file_list.list.as_ref()
            .and_then(|files| files.iter().find(|f| f.server_filename == filename))
            .ok_or_else(|| anyhow!("未找到远程文件: {}", remote_path))?;

        if file_info.isdir == 1 {
            return Err(anyhow!("{} 是目录，请使用 mget -r <目录> 下载", remote_path));
        }

        let metas = self.get_file_metas(&[file_info.fs_id]).await?;
        let file_meta = metas.list.first()
            .ok_or_else(|| anyhow!("获取下载链接失败"))?;

        let resume = check_resume(local_path, file_info.size);
        if resume == file_info.size {
            println!("文件已存在，跳过: {}", local_path);
            return Ok(());
        }
        let new_bytes = self.download_from_url_multithreaded(
            &file_meta.dlink, local_path, file_info.size, num_threads, resume,
        ).await?;
        let total = resume + new_bytes;
        if resume > 0 {
            println!("下载完成: {} (续传 {})", format_size(total), format_size(resume));
        } else {
            println!("下载成功: {} ({} 已写入)", format_size(total), format_size(new_bytes));
        }
        Ok(())
    }

    /// 多线程递归下载远程目录
    pub async fn download_dir_mt(
        &self,
        remote_dir: &str,
        local_dir: &str,
        num_threads: usize,
    ) -> anyhow::Result<()> {
        let all_entries = self.recursive_list(remote_dir).await?;
        let files: Vec<_> = all_entries.iter().filter(|f| f.isdir == 0).collect();

        if files.is_empty() {
            println!("目录为空，没有文件可下载");
            return Ok(());
        }

        println!("共 {} 个文件待下载", files.len());

        for (i, file_info) in files.iter().enumerate() {
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
            let lp = local_file_path.to_str().unwrap();
            let resume = check_resume(lp, file_info.size);
            if resume == file_info.size {
                println!("[{}/{}] 文件已存在，跳过: {}", i + 1, files.len(), lp);
                continue;
            }
            self.download_from_url_multithreaded(
                &file_meta.dlink, lp, file_info.size, num_threads, resume,
            ).await?;
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

/// 检查本地文件是否存在，返回已下载的字节数用于续传
/// 优先检查 .bftp_part 进度文件（多线程下载），其次检查文件大小（单线程下载）
fn check_resume(local_path: &str, remote_size: u64) -> u64 {
    let progress_path = format!("{}.bftp_part", local_path);
    if let Ok(mut f) = File::open(&progress_path) {
        let mut buf = [0u8; 8];
        if f.read_exact(&mut buf).is_ok() {
            let n = u64::from_le_bytes(buf);
            if n >= remote_size {
                std::fs::remove_file(&progress_path).ok();
                return remote_size;
            }
            return n;
        }
        std::fs::remove_file(&progress_path).ok();
    }
    // 无进度文件时，用文件大小判断（单线程下载未预分配）
    match std::fs::metadata(local_path) {
        Ok(meta) if meta.is_file() => {
            let local_size = meta.len();
            if local_size >= remote_size { remote_size } else { local_size }
        }
        _ => 0,
    }
}
