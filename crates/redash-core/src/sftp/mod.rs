use anyhow::{Context, Result};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use std::io::SeekFrom;
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use uuid::Uuid;

pub use redash_types::sftp::{FileCategory, PagedFileResult, RemoteFileItem};

pub struct SftpManager;

impl SftpManager {
    pub fn normalize_dir_path(dir_path: &str) -> String {
        let clean = dir_path.trim();
        if clean.is_empty() {
            "/".to_string()
        } else if clean.len() > 1 && clean.ends_with('/') {
            clean.trim_end_matches('/').to_string()
        } else {
            clean.to_string()
        }
    }

    pub async fn resolve_initial_dir(sftp: &SftpSession, fallback: &str) -> String {
        // 1. Try canonicalize(".") (POSIX SFTP realpath of authenticated working directory)
        if let Ok(path) = sftp.canonicalize(".").await {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // 2. Try canonicalize("")
        if let Ok(path) = sftp.canonicalize("").await {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // 3. Fallback to computed default path
        if !fallback.trim().is_empty() {
            fallback.to_string()
        } else {
            "/".to_string()
        }
    }

    pub async fn list_dir(sftp: &SftpSession, dir_path: &str) -> Result<Vec<RemoteFileItem>> {
        let target_dir = Self::normalize_dir_path(dir_path);

        let read_dir = sftp
            .read_dir(&target_dir)
            .await
            .with_context(|| format!("Failed to read directory at {}", target_dir))?;

        let mut items = Vec::new();
        for entry in read_dir {
            let name = entry.file_name();
            // Filter out current and parent directory relative pointers
            if name == "." || name == ".." {
                continue;
            }

            let metadata = entry.metadata();
            let is_dir = entry.file_type().is_dir();
            let is_symlink = entry.file_type().is_symlink();
            let size = metadata.len();
            let modified = metadata.modified().ok().and_then(|t| {
                t.duration_since(SystemTime::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            });
            let permissions = metadata.permissions.unwrap_or(0o644);

            items.push(RemoteFileItem {
                name,
                path: entry.path(),
                is_dir,
                is_symlink,
                size,
                modified,
                permissions,
            });
        }

        // Sort: directories first, then alphabetical by name
        items.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        Ok(items)
    }

    pub async fn list_dir_paged(
        sftp: &SftpSession,
        dir_path: &str,
        page: usize,
        page_size: usize,
        filter: Option<&str>,
    ) -> Result<PagedFileResult> {
        let all_items = Self::list_dir(sftp, dir_path).await?;
        Ok(PagedFileResult::paginate(
            all_items, page, page_size, filter,
        ))
    }

    pub async fn read_file_chunk(
        sftp: &SftpSession,
        path: &str,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Vec<u8>> {
        let mut file = sftp
            .open(path)
            .await
            .with_context(|| format!("Failed to open file {}", path))?;

        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .await
                .with_context(|| format!("Failed to seek to offset {} in {}", offset, path))?;
        }

        let mut buf = Vec::new();
        let mut chunk_reader = (&mut file).take(max_bytes as u64);
        chunk_reader
            .read_to_end(&mut buf)
            .await
            .with_context(|| format!("Failed to read bytes from {}", path))?;
        file.close()
            .await
            .context("Failed to close remote file after reading")?;
        Ok(buf)
    }

    pub async fn read_full_file(
        sftp: &SftpSession,
        path: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>> {
        let bytes = Self::read_file_chunk(sftp, path, 0, max_bytes.saturating_add(1)).await?;
        anyhow::ensure!(
            bytes.len() <= max_bytes,
            "File exceeds the {} byte editing limit; nothing was loaded for saving",
            max_bytes
        );
        Ok(bytes)
    }

    pub async fn write_file_atomic(sftp: &SftpSession, path: &str, data: &[u8]) -> Result<()> {
        Self::write_file_atomic_inner(sftp, None, path, data).await
    }

    pub async fn write_file_atomic_with_extension(
        sftp: &SftpSession,
        raw: &russh_sftp::client::RawSftpSession,
        path: &str,
        data: &[u8],
    ) -> Result<()> {
        Self::write_file_atomic_inner(sftp, Some(raw), path, data).await
    }

    async fn write_file_atomic_inner(
        sftp: &SftpSession,
        raw: Option<&russh_sftp::client::RawSftpSession>,
        path: &str,
        data: &[u8],
    ) -> Result<()> {
        let metadata = sftp
            .symlink_metadata(path)
            .await
            .context("Cannot read original file metadata")?;
        anyhow::ensure!(
            metadata
                .permissions
                .is_some_and(|mode| mode & 0o170000 == 0o100000),
            "Only regular files can be replaced; symbolic links must be edited at their resolved target"
        );
        let permissions = metadata
            .permissions
            .context("Server did not provide file permissions")?;
        let uid = metadata.uid.context("Server did not provide file owner")?;
        let gid = metadata.gid.context("Server did not provide file group")?;
        let tmp_path = format!("{}.redash_tmp_{}", path, Uuid::new_v4());

        let write_res = async {
            let mut file = sftp
                .open_with_flags_and_attributes(
                    &tmp_path,
                    OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
                    FileAttributes {
                        permissions: Some(0o600),
                        ..FileAttributes::default()
                    },
                )
                .await
                .with_context(|| format!("Failed to create temporary file at {}", tmp_path))?;
            file.write_all(data)
                .await
                .with_context(|| format!("Failed to write data to temporary file {}", tmp_path))?;
            file.flush()
                .await
                .with_context(|| format!("Failed to flush temporary file {}", tmp_path))?;
            file.close()
                .await
                .with_context(|| format!("Failed to close temporary file {}", tmp_path))?;
            // Ownership changes can clear setuid/setgid bits, so restore mode last.
            sftp.set_metadata(
                &tmp_path,
                FileAttributes {
                    uid: Some(uid),
                    gid: Some(gid),
                    ..FileAttributes::default()
                },
            )
            .await?;
            sftp.set_metadata(
                &tmp_path,
                FileAttributes {
                    permissions: Some(permissions),
                    ..FileAttributes::default()
                },
            )
            .await?;
            let saved = sftp.symlink_metadata(&tmp_path).await?;
            anyhow::ensure!(
                saved.permissions == Some(permissions)
                    && saved.uid == Some(uid)
                    && saved.gid == Some(gid),
                "Server failed to preserve file ownership or permissions"
            );
            Result::<()>::Ok(())
        }
        .await;

        if let Err(e) = write_res {
            let _ = sftp.remove_file(&tmp_path).await;
            return Err(e);
        }

        // Never unlink the destination to emulate replacement. Some SFTP v3 servers
        // refuse replacement; leave the complete temporary file available for recovery.
        let rename_result = if let Some(raw) = raw {
            // OpenSSH extension payload is two SSH length-prefixed strings.
            let mut payload = Vec::new();
            for value in [&tmp_path, path] {
                let len = u32::try_from(value.len()).context("Path too long")?;
                payload.extend_from_slice(&len.to_be_bytes());
                payload.extend_from_slice(value.as_bytes());
            }
            match raw.extended("posix-rename@openssh.com", payload).await {
                Ok(russh_sftp::protocol::Packet::Status(status))
                    if status.status_code == StatusCode::Ok =>
                {
                    Ok(())
                }
                Ok(russh_sftp::protocol::Packet::Status(status)) => {
                    Err(russh_sftp::client::error::Error::Status(status))
                }
                Ok(_) => Err(russh_sftp::client::error::Error::UnexpectedPacket),
                Err(error) => Err(error),
            }
        } else {
            sftp.rename(&tmp_path, path).await
        };
        if let Err(rename_err) = rename_result {
            return Err(rename_err).with_context(|| format!("Server could not safely replace {path}. Original retained; new content is at {tmp_path}"));
        }

        Ok(())
    }

    pub async fn create_file(sftp: &SftpSession, path: &str) -> Result<()> {
        let mut file = sftp
            .open_with_flags(
                path,
                OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            )
            .await
            .with_context(|| format!("Failed to create file at {}", path))?;
        file.flush().await?;
        file.close().await?;
        Ok(())
    }

    /// Inspect typed causes through anyhow's context chain, never the outer label.
    pub fn is_connection_error(error: &anyhow::Error) -> bool {
        use russh_sftp::client::error::Error;
        error.chain().any(|cause| {
            matches!(cause.downcast_ref::<Error>(), Some(Error::IO(_) | Error::Timeout | Error::UnexpectedBehavior(_)))
                || matches!(cause.downcast_ref::<Error>(), Some(Error::Status(status)) if matches!(status.status_code, StatusCode::NoConnection | StatusCode::ConnectionLost))
                || matches!(cause.downcast_ref::<std::io::Error>().map(std::io::Error::kind), Some(std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof))
        })
    }

    pub async fn chmod(sftp: &SftpSession, path: &str, mode: u32) -> Result<()> {
        let mut meta = sftp
            .metadata(path)
            .await
            .with_context(|| format!("Failed to get metadata for {}", path))?;
        meta.permissions = Some(mode);
        sftp.set_metadata(path, meta)
            .await
            .with_context(|| format!("Failed to set permissions on {}", path))?;
        Ok(())
    }

    pub async fn create_dir(sftp: &SftpSession, path: &str) -> Result<()> {
        sftp.create_dir(path)
            .await
            .with_context(|| format!("Failed to create directory at {}", path))?;
        Ok(())
    }

    pub async fn remove_entry(sftp: &SftpSession, path: &str, is_dir: bool) -> Result<()> {
        if is_dir {
            sftp.remove_dir(path)
                .await
                .with_context(|| format!("Failed to remove directory at {}", path))?;
        } else {
            sftp.remove_file(path)
                .await
                .with_context(|| format!("Failed to remove file at {}", path))?;
        }
        Ok(())
    }

    pub async fn rename(sftp: &SftpSession, old_path: &str, new_path: &str) -> Result<()> {
        sftp.rename(old_path, new_path)
            .await
            .with_context(|| format!("Failed to rename {} to {}", old_path, new_path))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_category_classification() {
        let dir_item = RemoteFileItem {
            name: "configs".to_string(),
            path: "/etc/configs".to_string(),
            is_dir: true,
            is_symlink: false,
            size: 4096,
            modified: Some(1727190000),
            permissions: 0o755,
        };
        assert_eq!(dir_item.category(), FileCategory::Directory);
        assert_eq!(dir_item.category().default_icon(), "📁");

        let symlink_item = RemoteFileItem {
            name: "current".to_string(),
            path: "/var/www/current".to_string(),
            is_dir: false,
            is_symlink: true,
            size: 16,
            modified: Some(1727190000),
            permissions: 0o777,
        };
        assert_eq!(symlink_item.category(), FileCategory::Symlink);
        assert_eq!(symlink_item.category().default_icon(), "🔗");

        let log_item = RemoteFileItem {
            name: "access.log".to_string(),
            path: "/var/log/nginx/access.log".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 102400,
            modified: Some(1727190000),
            permissions: 0o644,
        };
        assert_eq!(log_item.category(), FileCategory::Log);

        let config_item = RemoteFileItem {
            name: "config.toml".to_string(),
            path: "/etc/app/config.toml".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 512,
            modified: Some(1727190000),
            permissions: 0o644,
        };
        assert_eq!(config_item.category(), FileCategory::Config);

        let script_item = RemoteFileItem {
            name: "deploy.sh".to_string(),
            path: "/opt/deploy.sh".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 2048,
            modified: Some(1727190000),
            permissions: 0o755,
        };
        assert_eq!(script_item.category(), FileCategory::ScriptOrBinary);

        let exec_item = RemoteFileItem {
            name: "mydaemon".to_string(),
            path: "/usr/local/bin/mydaemon".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 5000000,
            modified: Some(1727190000),
            permissions: 0o755,
        };
        assert_eq!(exec_item.category(), FileCategory::ScriptOrBinary);

        let archive_item = RemoteFileItem {
            name: "backup.tar.gz".to_string(),
            path: "/backups/backup.tar.gz".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 10485760,
            modified: Some(1727190000),
            permissions: 0o644,
        };
        assert_eq!(archive_item.category(), FileCategory::Archive);

        let code_item = RemoteFileItem {
            name: "main.rs".to_string(),
            path: "/src/main.rs".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 1024,
            modified: Some(1727190000),
            permissions: 0o644,
        };
        assert_eq!(code_item.category(), FileCategory::Code);

        let doc_item = RemoteFileItem {
            name: "notes.txt".to_string(),
            path: "/home/user/notes.txt".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 128,
            modified: Some(1727190000),
            permissions: 0o644,
        };
        assert_eq!(doc_item.category(), FileCategory::Document);
    }

    #[test]
    fn test_remote_file_item_serialization() {
        let item = RemoteFileItem {
            name: "test.json".to_string(),
            path: "/tmp/test.json".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 256,
            modified: Some(1727191234),
            permissions: 0o644,
        };
        let json = serde_json::to_string(&item).unwrap();
        let decoded: RemoteFileItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, decoded);
    }

    #[test]
    fn test_paged_file_result_and_filtering() {
        let mut sample_items = Vec::new();
        for i in 1..=25 {
            sample_items.push(RemoteFileItem {
                name: format!("file_{:02}.log", i),
                path: format!("/var/log/file_{:02}.log", i),
                is_dir: false,
                is_symlink: false,
                size: (i * 100) as u64,
                modified: Some(1727190000 + i as u64),
                permissions: 0o644,
            });
        }

        // Page 1 with page_size 10
        let page1 = PagedFileResult::paginate(sample_items.clone(), 1, 10, None);
        assert_eq!(page1.total_items, 25);
        assert_eq!(page1.total_pages, 3);
        assert_eq!(page1.items.len(), 10);
        assert_eq!(page1.items[0].name, "file_01.log");

        // Page 3 with page_size 10 (last 5 items)
        let page3 = PagedFileResult::paginate(sample_items.clone(), 3, 10, None);
        assert_eq!(page3.items.len(), 5);
        assert_eq!(page3.items[4].name, "file_25.log");

        // Filter for "05"
        let filtered = PagedFileResult::paginate(sample_items, 1, 10, Some("05"));
        assert_eq!(filtered.total_items, 1);
        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].name, "file_05.log");
    }

    #[test]
    fn test_normalize_dir_path() {
        assert_eq!(SftpManager::normalize_dir_path(""), "/");
        assert_eq!(SftpManager::normalize_dir_path("   "), "/");
        assert_eq!(SftpManager::normalize_dir_path("/"), "/");
        assert_eq!(SftpManager::normalize_dir_path("/home/user/"), "/home/user");
        assert_eq!(SftpManager::normalize_dir_path("/home/user"), "/home/user");
        assert_eq!(
            SftpManager::normalize_dir_path("  /var/log/   "),
            "/var/log"
        );
    }
}
