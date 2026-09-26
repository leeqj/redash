use redash_core::sftp::SftpManager;
use russh_sftp::{
    client::{RawSftpSession, SftpSession},
    protocol::{Attrs, Data, FileAttributes, Handle, OpenFlags, Packet, Status, StatusCode},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
#[derive(Clone)]
struct MockFile {
    bytes: Vec<u8>,
    mode: u32,
}
#[derive(Default)]
struct MockFs {
    files: HashMap<String, MockFile>,
    fail_rename: bool,
    operations: Vec<String>,
}
struct MemorySftp(Arc<Mutex<MockFs>>);
fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: String::new(),
    }
}
impl russh_sftp::server::Handler for MemorySftp {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        attrs: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        fs.operations.push(format!("open {filename} {flags:?}"));
        if flags.contains(OpenFlags::CREATE | OpenFlags::EXCLUDE)
            && fs.files.contains_key(&filename)
        {
            return Err(StatusCode::Failure);
        }
        if flags.contains(OpenFlags::CREATE) {
            fs.files.entry(filename.clone()).or_insert(MockFile {
                bytes: Vec::new(),
                mode: attrs.permissions.unwrap_or(0o644) | 0o100000,
            });
        }
        let f = fs.files.get_mut(&filename).ok_or(StatusCode::NoSuchFile)?;
        if flags.contains(OpenFlags::TRUNCATE) {
            f.bytes.clear();
        }
        Ok(Handle {
            id,
            handle: filename,
        })
    }
    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let fs = self.0.lock().unwrap();
        let file = fs.files.get(&path).ok_or(StatusCode::NoSuchFile)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                permissions: Some(file.mode),
                uid: Some(1000),
                gid: Some(1000),
                size: Some(file.bytes.len() as u64),
                ..Default::default()
            },
        })
    }
    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        self.stat(id, path).await
    }
    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        let file = fs.files.get_mut(&path).ok_or(StatusCode::NoSuchFile)?;
        if let Some(mode) = attrs.permissions {
            file.mode = mode;
        }
        Ok(ok(id))
    }
    async fn extended(
        &mut self,
        id: u32,
        request: String,
        data: Vec<u8>,
    ) -> Result<Packet, StatusCode> {
        if request != "posix-rename@openssh.com" {
            return Err(StatusCode::OpUnsupported);
        }
        fn string(data: &mut &[u8]) -> String {
            let length = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
            let value = String::from_utf8(data[4..4 + length].to_vec()).unwrap();
            *data = &data[4 + length..];
            value
        }
        let mut data = data.as_slice();
        let old = string(&mut data);
        let new = string(&mut data);
        let mut fs = self.0.lock().unwrap();
        if fs.fail_rename {
            return Err(StatusCode::Failure);
        }
        let file = fs.files.remove(&old).ok_or(StatusCode::NoSuchFile)?;
        fs.files.insert(new, file);
        Ok(Packet::Status(ok(id)))
    }
    async fn close(&mut self, id: u32, _handle: String) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }
    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        let fs = self.0.lock().unwrap();
        let f = fs.files.get(&handle).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        if start >= f.bytes.len() {
            return Err(StatusCode::Eof);
        }
        let end = (start + len as usize).min(f.bytes.len());
        Ok(Data {
            id,
            data: f.bytes[start..end].to_vec(),
        })
    }
    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        let f = fs.files.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
        let offset = offset as usize;
        f.bytes.resize(f.bytes.len().max(offset + data.len()), 0);
        f.bytes[offset..offset + data.len()].copy_from_slice(&data);
        Ok(ok(id))
    }
    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        fs.operations.push(format!("remove {filename}"));
        fs.files.remove(&filename).ok_or(StatusCode::NoSuchFile)?;
        Ok(ok(id))
    }
    async fn rename(&mut self, id: u32, old: String, new: String) -> Result<Status, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        fs.operations.push(format!("rename {old} -> {new}"));
        if fs.fail_rename || fs.files.contains_key(&new) {
            return Err(StatusCode::Failure);
        }
        let file = fs.files.remove(&old).ok_or(StatusCode::NoSuchFile)?;
        fs.files.insert(new, file);
        Ok(ok(id))
    }
}

async fn sessions(
    bytes: Vec<u8>,
    mode: u32,
    fail_rename: bool,
) -> (SftpSession, RawSftpSession, Arc<Mutex<MockFs>>) {
    let fs = Arc::new(Mutex::new(MockFs {
        files: HashMap::from([("/config".into(), MockFile { bytes, mode })]),
        fail_rename,
        operations: vec![],
    }));
    let (client, server) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(russh_sftp::server::run(server, MemorySftp(fs.clone())));
    let sftp = SftpSession::new(client).await.unwrap();
    let (client, server) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(russh_sftp::server::run(server, MemorySftp(fs.clone())));
    let raw = RawSftpSession::new(client);
    raw.init().await.unwrap();
    (sftp, raw, fs)
}

#[tokio::test]
async fn complete_editor_read_preserves_large_files_and_permissions() {
    let content = vec![b'x'; 1024 * 1024];
    let (sftp, raw, fs) = sessions(content.clone(), 0o100700, false).await;
    assert!(
        SftpManager::read_full_file(&sftp, "/config", 512 * 1024)
            .await
            .is_err()
    );
    let bytes = SftpManager::read_full_file(&sftp, "/config", 2 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(bytes, content);
    SftpManager::write_file_atomic_with_extension(&sftp, &raw, "/config", &bytes)
        .await
        .unwrap();
    let files = fs.lock().unwrap();
    assert_eq!(files.files["/config"].bytes, content);
    assert_eq!(files.files["/config"].mode, 0o100700);
    assert_eq!(files.files.len(), 1);
}

#[tokio::test]
async fn failed_rename_retains_original_and_recovery_copy() {
    let (sftp, raw, fs) = sessions(b"original".to_vec(), 0o100600, true).await;
    let error =
        SftpManager::write_file_atomic_with_extension(&sftp, &raw, "/config", b"replacement")
            .await
            .unwrap_err();
    assert!(error.to_string().contains("Original retained"));
    let files = fs.lock().unwrap();
    assert_eq!(files.files["/config"].bytes, b"original");
    assert!(
        files
            .files
            .iter()
            .any(|(name, file)| name.contains(".redash_tmp_") && file.bytes == b"replacement")
    );
    assert!(!files.operations.iter().any(|op| op == "remove /config"));
}

#[tokio::test]
async fn create_is_exclusive_and_symlink_edit_is_rejected() {
    let (sftp, _, fs) = sessions(b"original".to_vec(), 0o100600, false).await;
    assert!(SftpManager::create_file(&sftp, "/config").await.is_err());
    assert_eq!(fs.lock().unwrap().files["/config"].bytes, b"original");
    fs.lock().unwrap().files.get_mut("/config").unwrap().mode = 0o120777;
    assert!(
        SftpManager::write_file_atomic(&sftp, "/config", b"replacement")
            .await
            .is_err()
    );
    assert_eq!(fs.lock().unwrap().files["/config"].bytes, b"original");
}

#[test]
fn connection_error_is_found_under_context() {
    let error = anyhow::Error::new(russh_sftp::client::error::Error::IO("closed".into()))
        .context("Failed to read directory");
    assert!(SftpManager::is_connection_error(&error));
    let error = anyhow::Error::new(russh_sftp::client::error::Error::Status(Status {
        status_code: StatusCode::PermissionDenied,
        ..ok(0)
    }))
    .context("Failed to read directory");
    assert!(!SftpManager::is_connection_error(&error));
}
