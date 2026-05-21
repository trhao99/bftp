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

use crate::constants::*;
use crate::display::format_size;
use crate::error::BftpError;
use crate::models::*;
use crate::session::Session;

pub type MsgCallback = dyn Fn(&str) + Send + Sync;

pub fn new_progress_bar(file_size: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(file_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{wide_bar} {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
            .unwrap()
    );
    pb.set_message(message.to_string());
    pb
}

/// 下载策略
#[derive(Debug, Clone, Copy)]
pub struct DownloadOptions {
    pub num_threads: usize,
    pub resume: u64,
}

impl DownloadOptions {
    pub fn single(resume: u64) -> Self {
        Self { num_threads: 1, resume }
    }
    pub fn multi(resume: u64, num_threads: usize) -> Self {
        Self { num_threads, resume }
    }
}

/// 百度网盘API客户端
pub struct BaiduApiClient {
    client: Client,
    pub session: Session,
}

impl BaiduApiClient {
    pub fn new(access_token: String) -> Self {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            session: Session::new(access_token),
        }
    }

    // ---- API 方法 ----

    fn api_url(&self, path: &str) -> String {
        format!("{}{}&access_token={}", PAN_BAIDU_API_BASE, path, self.session.access_token)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> anyhow::Result<T> {
        let response = self.client.get(url).send().await?;
        Ok(response.json().await?)
    }

    async fn post_form<T: serde::de::DeserializeOwned>(&self, url: &str, body: String) -> anyhow::Result<T> {
        let response = self.client.post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await?;
        Ok(response.json().await?)
    }

    fn check<T: ApiResponse>(result: T, context: &str) -> anyhow::Result<T> {
        if !result.is_success() {
            let api_err = BftpError::Api {
                errno: result.error_code(),
                errmsg: result.error_msg(),
            };
            return Err(anyhow::Error::from(api_err).context(context.to_string()));
        }
        Ok(result)
    }

    pub async fn get_user_info(&self) -> anyhow::Result<UserInfoResponse> {
        let url = self.api_url("/xpan/nas?method=uinfo&vip_version=v2");
        self.get_json(&url).await
    }

    pub async fn get_remote_current_path_files_info(&self) -> anyhow::Result<FileListResponse> {
        let url = format!(
            "{}&dir={}",
            self.api_url("/xpan/file?method=list"),
            self.session.current_remote_path
        );
        let result: FileListResponse = self.get_json(&url).await?;
        Self::check(result, "列出文件失败")
    }

    pub async fn verify_token(&self) -> bool {
        if let Ok(info) = self.get_user_info().await {
            info.base.errno == 0
        } else {
            false
        }
    }

    pub async fn list_files_in_dir(&self, dir: &str) -> anyhow::Result<FileListResponse> {
        let url = format!("{}&dir={}", self.api_url("/xpan/file?method=list"), dir);
        let result: FileListResponse = self.get_json(&url).await?;
        Self::check(result, "列出文件失败")
    }

    pub async fn get_file_metas(&self, fsids: &[u64]) -> anyhow::Result<QueryFileInfoResponse> {
        let fsids_json = serde_json::to_string(fsids)?;
        let url = format!(
            "{}/xpan/multimedia?method=filemetas&access_token={}&fsids={}&dlink=1",
            PAN_BAIDU_API_BASE, self.session.access_token, fsids_json
        );
        let result: QueryFileInfoResponse = self.get_json(&url).await?;
        let result = Self::check(result, "获取文件元信息失败")?;
        if result.list.is_empty() {
            return Err(anyhow!("未获取到文件元信息"));
        }
        Ok(result)
    }

    pub async fn recursive_list(&self, path: &str) -> anyhow::Result<Vec<FileInfo>> {
        let encoded_path = urlencoding::encode(path);
        let mut all_files: Vec<FileInfo> = Vec::new();
        let mut cursor = 0i32;

        loop {
            let url = format!(
                "{}/xpan/multimedia?method=listall&access_token={}&path={}&recursion=1&start={}&limit=1000",
                PAN_BAIDU_API_BASE, self.session.access_token, encoded_path, cursor
            );
            let result: CategoryFileListResponse = self.get_json(&url).await?;
            let result = Self::check(result, "递归列出文件失败")?;
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

    // ---- 下载 ----

    /// 解析远程文件的 dlink（通过路径查找 fs_id → dlink）
    async fn resolve_download_link(&self, remote_path: &str) -> anyhow::Result<(FileMeta, u64)> {
        let p = Path::new(remote_path);
        let parent_dir = p.parent().and_then(|x| x.to_str()).unwrap_or("/");
        let filename = p.file_name().and_then(|x| x.to_str()).unwrap_or("");

        let file_list = self.list_files_in_dir(parent_dir).await?;
        let file_info = file_list.list.as_ref()
            .and_then(|files| files.iter().find(|f| f.server_filename == filename))
            .ok_or_else(|| anyhow!("未找到远程文件: {}", remote_path))?;

        if file_info.isdir == 1 {
            return Err(anyhow!("{} 是目录，请使用 -r 下载", remote_path));
        }

        let metas = self.get_file_metas(&[file_info.fs_id]).await?;
        let file_meta = metas.list.into_iter().next()
            .ok_or_else(|| anyhow!("获取下载链接失败"))?;
        Ok((file_meta, file_info.size))
    }

    /// 通过 dlink 下载文件（单线程），返回新下载的字节数
    pub async fn download_from_url(&self, dlink: &str, local_path: &str, _file_size: u64, resume: u64, pb: Option<ProgressBar>) -> anyhow::Result<u64> {
        let url = format!("{}&access_token={}", dlink, self.session.access_token);
        let mut req = self.client.get(&url)
            .header("User-Agent", USER_AGENT);
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

        if let Some(ref pb) = pb
            && resume > 0
        {
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
            if let Some(ref pb) = pb {
                pb.set_position(downloaded);
            }
        }
        if let Some(ref pb) = pb {
            pb.finish_and_clear();
        }

        Ok(downloaded - resume)
    }

    /// 多线程下载，返回新下载的字节数
    pub async fn download_from_url_multithreaded(
        &self,
        dlink: &str,
        local_path: &str,
        file_size: u64,
        num_threads: usize,
        resume: u64,
        pb: Option<ProgressBar>,
    ) -> anyhow::Result<u64> {
        if num_threads <= 1 || file_size.saturating_sub(resume) < MULTITHREAD_MIN_SIZE {
            return self.download_from_url(dlink, local_path, file_size, resume, pb).await;
        }

        let url = format!("{}&access_token={}", dlink, self.session.access_token);

        if let Some(parent) = Path::new(local_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = if resume == 0 {
            File::create(local_path)?
        } else {
            OpenOptions::new().write(true).open(local_path)?
        };
        file.set_len(file_size)?;

        let progress_path = format!("{}.bftp_part", local_path);
        let total = Arc::new(AtomicU64::new(resume));
        File::create(&progress_path)?.write_at(&resume.to_le_bytes(), 0)?;

        let pb = pb.map(Arc::new);
        if let Some(ref pb) = pb
            && resume > 0
        {
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
                    .header("User-Agent", USER_AGENT)
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
                    if let Some(ref pb) = pb {
                        pb.inc(chunk.len() as u64);
                    }
                    let n = total.fetch_add(chunk.len() as u64, Ordering::Relaxed) + chunk.len() as u64;
                    if let Ok(pf) = OpenOptions::new().write(true).open(&progress_path) {
                        pf.write_at(&n.to_le_bytes(), 0)?;
                    }
                }
                Ok::<_, anyhow::Error>(offset - start)
            }));
        }

        let mut downloaded: u64 = 0;
        for handle in handles {
            let size = handle.await.context("分片任务失败")??;
            downloaded += size;
        }
        if let Some(ref pb) = pb {
            pb.finish_and_clear();
        }
        std::fs::remove_file(&progress_path).ok();

        Ok(downloaded)
    }

    /// 统一下载入口：根据 DownloadOptions 选择单线程/多线程
    async fn download_from_url_auto(
        &self,
        dlink: &str,
        local_path: &str,
        file_size: u64,
        opts: DownloadOptions,
        pb: Option<ProgressBar>,
    ) -> anyhow::Result<u64> {
        if opts.num_threads <= 1 || file_size.saturating_sub(opts.resume) < MULTITHREAD_MIN_SIZE {
            self.download_from_url(dlink, local_path, file_size, opts.resume, pb).await
        } else {
            self.download_from_url_multithreaded(dlink, local_path, file_size, opts.num_threads, opts.resume, pb).await
        }
    }

    /// 下载单个文件（单线程），通过远程路径
    pub async fn download_file(&self, remote_path: &str, local_path: &str) -> anyhow::Result<()> {
        self.download_file_with_opts(remote_path, local_path, DownloadOptions::single(0)).await
    }

    /// 多线程下载单个文件
    pub async fn download_file_mt(&self, remote_path: &str, local_path: &str, num_threads: usize) -> anyhow::Result<()> {
        let resume = check_resume(local_path, 0);
        self.download_file_with_opts(remote_path, local_path, DownloadOptions::multi(resume, num_threads)).await
    }

    /// 统一的单文件下载
    async fn download_file_with_opts(&self, remote_path: &str, local_path: &str, opts: DownloadOptions) -> anyhow::Result<()> {
        let (file_meta, file_size) = self.resolve_download_link(remote_path).await?;

        let resume = if opts.resume > 0 {
            opts.resume
        } else {
            check_resume(local_path, file_size)
        };

        if resume == file_size {
            println!("文件已存在，跳过: {}", local_path);
            return Ok(());
        }

        let mut opts = opts;
        opts.resume = resume;

        let pb = new_progress_bar(file_size, local_path);
        let new_bytes = self.download_from_url_auto(&file_meta.dlink, local_path, file_size, opts, Some(pb)).await?;
        let total = resume + new_bytes;
        if resume > 0 {
            println!("下载完成: {} {} (续传 {})", local_path, format_size(total), format_size(resume));
        } else {
            println!("下载成功: {} ({})", local_path, format_size(new_bytes));
        }
        Ok(())
    }

    /// 递归下载远程目录（单线程，增量）
    pub async fn download_dir(&self, remote_dir: &str, local_dir: &str) -> anyhow::Result<()> {
        self.download_dir_with_opts(remote_dir, local_dir, DownloadOptions::single(0)).await
    }

    /// 多线程递归下载远程目录（增量）
    pub async fn download_dir_mt(&self, remote_dir: &str, local_dir: &str, num_threads: usize) -> anyhow::Result<()> {
        self.download_dir_with_opts(remote_dir, local_dir, DownloadOptions::multi(0, num_threads)).await
    }

    /// 统一的目录下载
    async fn download_dir_with_opts(&self, remote_dir: &str, local_dir: &str, opts: DownloadOptions) -> anyhow::Result<()> {
        let all_entries = self.recursive_list(remote_dir).await?;
        let files: Vec<_> = all_entries.iter().filter(|f| f.isdir == 0).collect();

        if files.is_empty() {
            println!("目录为空，没有文件可下载");
            return Ok(());
        }

        // 预扫描
        let mut skip_count = 0u64;
        let mut skip_size = 0u64;
        let mut new_count = 0u64;
        let mut new_size = 0u64;
        for file_info in &files {
            let rel_path = file_info.path.strip_prefix(remote_dir).unwrap_or(&file_info.path);
            let rel_path = rel_path.strip_prefix('/').unwrap_or(rel_path);
            let local_file_path = Path::new(local_dir).join(rel_path);
            let lp = local_file_path.to_str().unwrap();
            let resume = check_resume(lp, file_info.size);
            if resume == file_info.size {
                skip_count += 1;
                skip_size += file_info.size;
            } else {
                new_count += 1;
                new_size += file_info.size;
            }
        }

        if new_count == 0 {
            println!("共 {} 个文件，全部已存在本地，无需下载", files.len());
            return Ok(());
        }

        println!(
            "共 {} 个文件，{} 个已存在本地（{}），将下载 {} 个新文件（{}）",
            files.len(), skip_count, format_size(skip_size), new_count, format_size(new_size),
        );

        let mut downloaded = 0u64;
        let total = files.len();
        for (i, file_info) in files.iter().enumerate() {
            let rel_path = file_info.path.strip_prefix(remote_dir).unwrap_or(&file_info.path);
            let rel_path = rel_path.strip_prefix('/').unwrap_or(rel_path);
            let local_file_path = Path::new(local_dir).join(rel_path);
            let lp = local_file_path.to_str().unwrap();

            let mut file_opts = opts;
            file_opts.resume = check_resume(lp, file_info.size);
            if file_opts.resume == file_info.size {
                continue;
            }

            println!("[{}/{}] 下载: {} -> {}", i + 1, total, file_info.path, local_file_path.display());

            let metas = self.get_file_metas(&[file_info.fs_id]).await?;
            let file_meta = metas.list.first()
                .ok_or_else(|| anyhow!("获取下载链接失败"))?;
            let pb = new_progress_bar(file_info.size, lp);
            self.download_from_url_auto(&file_meta.dlink, lp, file_info.size, file_opts, Some(pb)).await?;
            downloaded += 1;
        }

        println!("下载完成: 已下载 {} 个新文件，跳过 {} 个已有文件", downloaded, skip_count);
        Ok(())
    }

    // ---- 上传 ----

    pub async fn precreate(&self, path: &str, size: u64, block_list: &str, uploadid: Option<&str>) -> anyhow::Result<PrecreateResponse> {
        let url = self.api_url("/xpan/file?method=precreate");
        let mut body = format!(
            "path={}&size={}&isdir=0&block_list={}&autoinit=1&rtype=1",
            urlencoding::encode(path), size, urlencoding::encode(block_list),
        );
        if let Some(uid) = uploadid {
            body.push_str(&format!("&uploadid={}", urlencoding::encode(uid)));
        }
        let result: PrecreateResponse = self.post_form(&url, body).await?;
        Self::check(result, "预上传失败")
    }

    pub async fn locate_upload(&self, path: &str, uploadid: &str) -> anyhow::Result<String> {
        let encoded_path = urlencoding::encode(path);
        let url = format!(
            "{}/file?method=locateupload&appid={}&access_token={}&path={}&uploadid={}&upload_version={}",
            D_PCS_BAIDU_BASE, APP_ID, self.session.access_token, encoded_path, uploadid, UPLOAD_VERSION
        );
        let result: LocateUploadResponse = self.get_json(&url).await?;
        let result = Self::check(result, "获取上传域名失败")?;
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

    pub async fn upload_chunk(
        &self, domain: &str, path: &str, uploadid: &str, partseq: i32, data: Vec<u8>,
    ) -> anyhow::Result<String> {
        let encoded_path = urlencoding::encode(path);
        let url = format!(
            "{}{}?method=upload&access_token={}&type=tmpfile&path={}&uploadid={}&partseq={}",
            domain, PCS_UPLOAD_PATH, self.session.access_token, encoded_path, uploadid, partseq
        );

        let part = reqwest::multipart::Part::bytes(data)
            .file_name("blob")
            .mime_str("application/octet-stream")
            .context("设置 MIME 类型失败")?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let response = self.client.post(&url)
            .multipart(form)
            .send()
            .await?;
        let status = response.status();
        let body_text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("分片上传HTTP错误: status={}, body={}", status, body_text));
        }
        let result: UploadChunkResponse = serde_json::from_str(&body_text)
            .map_err(|e| anyhow!("解析分片上传响应失败: {}, body={}", e, body_text))?;
        if !result.is_success() {
            return Err(anyhow!("分片上传失败: {}, body={}", result.error_desc(), body_text));
        }
        Ok(result.md5.unwrap_or_default())
    }

    pub async fn create_file(&self, path: &str, size: u64, block_list: &str, uploadid: &str) -> anyhow::Result<CreateFileResponse> {
        let url = self.api_url("/xpan/file?method=create");
        let body = format!(
            "path={}&size={}&isdir=0&block_list={}&uploadid={}&rtype=1",
            urlencoding::encode(path), size, urlencoding::encode(block_list), urlencoding::encode(uploadid),
        );
        let result: CreateFileResponse = self.post_form(&url, body).await?;
        Self::check(result, "创建文件失败")
    }

    pub async fn upload_file(&self, local_path: &str, remote_filename: Option<&str>, on_msg: Option<&MsgCallback>) -> anyhow::Result<()> {
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

        let remote_path = if self.session.current_remote_path.ends_with('/') {
            format!("{}{}", self.session.current_remote_path, filename)
        } else {
            format!("{}/{}", self.session.current_remote_path, filename)
        };

        let msg = |s: &str| {
            if let Some(cb) = on_msg {
                cb(s);
            }
        };

        msg(&format!("上传文件: {} -> {}", local_path, remote_path));

        let (file_size, block_list, chunk_count) = compute_block_list(local_path)?;
        msg(&format!("文件大小: {}, 分片数: {}", format_size(file_size), chunk_count));

        // 读取上传断点进度
        let progress_path = format!("{}.bftp_upload", local_path);
        let (saved_uploadid, saved_chunks) = read_upload_progress(&progress_path);

        let resume_info = if saved_chunks.is_empty() {
            String::new()
        } else {
            format!("（续传，已完成 {}/{} 分片）", saved_chunks.len(), chunk_count)
        };

        msg(&format!("[1/3] 预上传...{}", resume_info));
        let precreate_result = self.precreate(&remote_path, file_size, &block_list, saved_uploadid.as_deref()).await?;
        let uploadid = precreate_result.uploadid.context("预上传未返回uploadid")?;
        let chunks_to_upload = precreate_result.block_list.unwrap_or_else(|| vec![0]);
        msg(&format!("uploadid: {}", uploadid));

        // 保存/更新 uploadid
        save_upload_progress(&progress_path, &uploadid, &[])?;

        msg("[2/3] 获取上传域名...");
        let domain = self.locate_upload(&remote_path, &uploadid).await?;
        msg(&format!("上传域名: {}", domain));

        let total_chunks = chunks_to_upload.len();
        if total_chunks == 0 {
            msg("所有分片已上传，跳过上传步骤");
        }
        for (i, &chunk_idx) in chunks_to_upload.iter().enumerate() {
            let progress = format!("{}/{}", i + 1, total_chunks);
            msg(&format!("[2/3] 上传分片 {} (index={})...", progress, chunk_idx));
            let chunk_data = read_chunk(local_path, chunk_idx as usize)?;
            let chunk_md5 = self.upload_chunk(&domain, &remote_path, &uploadid, chunk_idx, chunk_data).await?;
            msg(&format!("  分片 {} md5: {}", chunk_idx, chunk_md5));
            // 记录已完成的分片
            append_completed_chunk(&progress_path, chunk_idx)?;
        }

        msg("[3/3] 创建文件...");
        let result = self.create_file(&remote_path, file_size, &block_list, &uploadid).await?;
        msg("上传成功!");
        msg(&format!("  文件名: {}", result.server_filename.as_deref().unwrap_or(filename)));
        msg(&format!("  路径: {}", result.path.as_deref().unwrap_or(&remote_path)));
        msg(&format!("  大小: {}", format_size(result.size.unwrap_or(file_size))));
        msg(&format!("  fs_id: {}", result.fs_id.unwrap_or(0)));

        // 清理断点文件
        std::fs::remove_file(&progress_path).ok();
        Ok(())
    }

    // ---- 文件管理 ----

    pub async fn filemanager(&self, opera: &str, filelist: &str) -> anyhow::Result<FileManagerResponse> {
        let url = format!(
            "{}&opera={}&async=0",
            self.api_url("/xpan/file?method=filemanager"), opera
        );
        let body = format!("filelist={}", urlencoding::encode(filelist));
        let result: FileManagerResponse = self.post_form(&url, body).await?;
        let result = Self::check(result, "文件操作失败")?;
        if let Some(ref info) = result.info {
            for item in info {
                if item.errno != 0 {
                    return Err(anyhow!("文件 {} 操作失败: errno={}", item.path.as_deref().unwrap_or("?"), item.errno));
                }
            }
        }
        Ok(result)
    }

    pub async fn rename_file(&self, path: &str, newname: &str) -> anyhow::Result<()> {
        let filelist = serde_json::to_string(&[serde_json::json!({
            "path": path,
            "newname": newname
        })])?;
        self.filemanager("rename", &filelist).await?;
        Ok(())
    }

    pub async fn copy_file(&self, path: &str, dest: &str, newname: &str) -> anyhow::Result<()> {
        let filelist = serde_json::to_string(&[serde_json::json!({
            "path": path,
            "dest": dest,
            "newname": newname
        })])?;
        self.filemanager("copy", &filelist).await?;
        Ok(())
    }

    pub async fn delete_file(&self, path: &str) -> anyhow::Result<()> {
        let filelist = serde_json::to_string(&[path])?;
        self.filemanager("delete", &filelist).await?;
        Ok(())
    }

    pub async fn create_remote_dir(&self, path: &str) -> anyhow::Result<CreateFileResponse> {
        let url = self.api_url("/xpan/file?method=create");
        let body = format!("path={}&isdir=1&rtype=0", urlencoding::encode(path));
        let result: CreateFileResponse = self.post_form(&url, body).await?;
        Self::check(result, "创建远程目录失败")
    }

    // ---- 搜索 ----

    pub async fn search_files_by_keyword(&self, key: &str, dir: Option<&str>, recursion: bool) -> anyhow::Result<SearchFileByKeywordResponse> {
        let encoded_key = urlencoding::encode(key);
        let mut url = format!(
            "{}/xpan/file?method=search&access_token={}&key={}",
            PAN_BAIDU_API_BASE, self.session.access_token, encoded_key
        );
        if let Some(d) = dir {
            url.push_str(&format!("&dir={}", urlencoding::encode(d)));
        } else {
            url.push_str(&format!("&dir={}", urlencoding::encode(&self.session.current_remote_path)));
        }
        if recursion {
            url.push_str("&recursion=1");
        }
        let response = self.client.get(&url)
            .header("User-Agent", USER_AGENT)
            .send().await?;
        let result: SearchFileByKeywordResponse = response.json().await?;
        Self::check(result, "关键字搜索失败")
    }

    pub async fn search_files_semantic(&self, query: &str, search_type: i32, dir: Option<&str>) -> anyhow::Result<SearchFileBySemanticResponse> {
        let dir_val = dir.unwrap_or(&self.session.current_remote_path);
        let url = format!(
            "https://pan.baidu.com/xpan/unisearch?access_token={}&scene=mcpserver&query={}&search_type={}&num=500&dir={}",
            self.session.access_token,
            urlencoding::encode(query),
            search_type,
            urlencoding::encode(dir_val),
        );
        let response = self.client.post(&url)
            .header("Content-Type", "application/json")
            .body("{}")
            .send().await?;
        let result: SearchFileBySemanticResponse = response.json().await?;
        Self::check(result, "语义搜索失败")
    }

    pub async fn get_capacity_info(&self) -> anyhow::Result<CapacityInfoResponse> {
        let url = format!(
            "https://pan.baidu.com/api/quota?access_token={}",
            self.session.access_token
        );
        let result: CapacityInfoResponse = self.get_json(&url).await?;
        Self::check(result, "获取容量信息失败")
    }
}

// ==================== 上传辅助函数 ====================

fn compute_file_md5(data: &[u8]) -> String {
    format!("{:x}", md5::compute(data))
}

fn compute_block_list(file_path: &str) -> anyhow::Result<(u64, String, usize)> {
    let mut file = File::open(file_path)?;
    let file_size = file.metadata()?.len();

    let mut md5s: Vec<String> = Vec::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        md5s.push(compute_file_md5(&buffer[..bytes_read]));
    }

    if md5s.is_empty() {
        md5s.push(compute_file_md5(&[]));
    }

    let block_list_str = serde_json::to_string(&md5s)?;
    Ok((file_size, block_list_str, md5s.len()))
}

fn read_chunk(file_path: &str, chunk_index: usize) -> anyhow::Result<Vec<u8>> {
    let mut file = File::open(file_path)?;
    let offset = chunk_index as u64 * CHUNK_SIZE_U64;
    file.seek(SeekFrom::Start(offset))?;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let bytes_read = file.read(&mut buffer)?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

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
    match std::fs::metadata(local_path) {
        Ok(meta) if meta.is_file() => {
            let local_size = meta.len();
            if local_size >= remote_size { remote_size } else { local_size }
        }
        _ => 0,
    }
}

fn read_upload_progress(progress_path: &str) -> (Option<String>, Vec<i32>) {
    let content = match std::fs::read_to_string(progress_path) {
        Ok(c) => c,
        Err(_) => return (None, Vec::new()),
    };
    let mut lines = content.lines();
    let uploadid = lines.next().map(|s| s.to_string());
    let chunks: Vec<i32> = lines
        .filter_map(|l| l.trim().parse().ok())
        .collect();
    (uploadid, chunks)
}

fn save_upload_progress(progress_path: &str, uploadid: &str, _chunks: &[i32]) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(progress_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(progress_path, format!("{}\n", uploadid))?;
    Ok(())
}

fn append_completed_chunk(progress_path: &str, chunk_idx: i32) -> anyhow::Result<()> {
    use std::io::Write;
    let mut file = OpenOptions::new().append(true).open(progress_path)?;
    writeln!(file, "{}", chunk_idx)?;
    Ok(())
}
