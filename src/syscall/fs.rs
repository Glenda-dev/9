use crate::nine::user::UserAccessSession;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use glenda::cap::Endpoint;
use glenda::client::FsClient;
use glenda::error::Error;
use glenda::interface::{CSpaceService, FileHandleService, FileSystemService};
use glenda::ipc::Badge;
use glenda::protocol::fs::OpenFlags;

impl<'a, 'b> UserAccessSession<'a, 'b> {
    pub fn sys_open(&mut self, sp: usize) -> Result<usize, Error> {
        let name_ptr = self.read_user_usize(sp + 8)?;
        let mode = self.read_user_usize(sp + 16)?;
        let name = self.strncpy_from_user(name_ptr, 1024)?;

        debug!("Nine: sys_open(\"{}\", {:#x})", name, mode);

        let flags = match mode & 3 {
            0 => OpenFlags::O_RDONLY,
            1 => OpenFlags::O_WRONLY,
            2 => OpenFlags::O_RDWR,
            3 => OpenFlags::O_RDONLY,
            _ => OpenFlags::O_RDONLY,
        };

        let fd_slot = self.mgr.cspace_mgr.alloc(&mut *self.mgr.res_client)?;
        self.mgr.fs_client.open(Badge::null(), &name, flags, 0o644, fd_slot)?;

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let fd = task.files.open(FsClient::new(Endpoint::from(fd_slot)), fd_slot, name);

        Ok(fd as usize)
    }

    pub fn sys_read(&mut self, sp: usize) -> Result<usize, Error> {
        let fd = self.read_user_usize(sp + 8)? as u32;
        let buf_ptr = self.read_user_usize(sp + 16)?;
        let n = self.read_user_usize(sp + 24)?;

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let mut handle = task.files.get(fd).ok_or(Error::InvalidArgs)?;

        let mut data = vec![0u8; n];
        let read_len = handle.fs_client.read(Badge::null(), handle.offset, &mut data)?;

        self.copy_to_user(buf_ptr, &data[..read_len])?;

        handle.offset += read_len;
        {
            let mut files = task.files.state.write();
            if let Some(h) = files.fds.get_mut(&fd) {
                h.offset = handle.offset;
            }
        }

        Ok(read_len)
    }

    pub fn sys_pread(&mut self, sp: usize) -> Result<usize, Error> {
        let fd = self.read_user_usize(sp + 8)? as u32;
        let buf_ptr = self.read_user_usize(sp + 16)?;
        let n = self.read_user_usize(sp + 24)?;
        let offset = self.read_user_usize(sp + 32)?;

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let mut handle = task.files.get(fd).ok_or(Error::InvalidArgs)?;

        let mut data = vec![0u8; n];
        let read_len = handle.fs_client.read(Badge::null(), offset, &mut data)?;

        self.copy_to_user(buf_ptr, &data[..read_len])?;

        Ok(read_len)
    }

    pub fn sys_write(&mut self, sp: usize) -> Result<usize, Error> {
        let fd = self.read_user_usize(sp + 8)? as u32;
        let buf_ptr = self.read_user_usize(sp + 16)?;
        let n = self.read_user_usize(sp + 24)?;

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let mut handle = task.files.get(fd).ok_or(Error::InvalidArgs)?;

        let mut data = vec![0u8; n];
        self.copy_from_user(buf_ptr, &mut data)?;

        let write_len = handle.fs_client.write(Badge::null(), handle.offset, &data)?;

        handle.offset += write_len;
        {
            let mut files = task.files.state.write();
            if let Some(h) = files.fds.get_mut(&fd) {
                h.offset = handle.offset;
            }
        }

        Ok(write_len)
    }

    pub fn sys_pwrite(&mut self, sp: usize) -> Result<usize, Error> {
        let fd = self.read_user_usize(sp + 8)? as u32;
        let buf_ptr = self.read_user_usize(sp + 16)?;
        let n = self.read_user_usize(sp + 24)?;
        let offset = self.read_user_usize(sp + 32)?;

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let mut handle = task.files.get(fd).ok_or(Error::InvalidArgs)?;

        let mut data = vec![0u8; n];
        self.copy_from_user(buf_ptr, &mut data)?;

        let write_len = handle.fs_client.write(Badge::null(), offset, &data)?;

        Ok(write_len)
    }

    pub fn sys_seek(&mut self, sp: usize) -> Result<usize, Error> {
        let v_ptr = self.read_user_usize(sp + 8)?;
        let fd = self.read_user_usize(sp + 16)? as u32;
        let n = self.read_user_usize(sp + 24)? as i64;
        let t = self.read_user_usize(sp + 32)?;

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let mut handle = task.files.get(fd).ok_or(Error::InvalidArgs)?;

        let new_offset = handle.fs_client.seek(Badge::null(), n, t)?;

        {
            let mut files = task.files.state.write();
            if let Some(h) = files.fds.get_mut(&fd) {
                h.offset = new_offset;
            }
        }

        let bytes = new_offset.to_ne_bytes();
        self.copy_to_user(v_ptr, &bytes)?;

        Ok(0)
    }

    pub fn sys_close(&mut self, sp: usize) -> Result<usize, Error> {
        let fd = self.read_user_usize(sp + 8)? as u32;
        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;

        if let Some(mut handle) = task.files.get(fd) {
            let _ = handle.fs_client.close(Badge::null());
        }

        task.files.close(fd);
        Ok(0)
    }

    pub fn sys_fd2path(&mut self, sp: usize) -> Result<usize, Error> {
        let fd = self.read_user_usize(sp + 8)? as u32;
        let buf_ptr = self.read_user_usize(sp + 16)?;
        let n_buf = self.read_user_usize(sp + 24)?;

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let handle = task.files.get(fd).ok_or(Error::InvalidArgs)?;

        let path_bytes = handle.path.as_bytes();
        let copy_len = core::cmp::min(path_bytes.len(), n_buf - 1);
        self.copy_to_user(buf_ptr, &path_bytes[..copy_len])?;
        self.copy_to_user(buf_ptr + copy_len, &[0u8])?;

        Ok(0)
    }

    pub fn sys_stat(&mut self, sp: usize) -> Result<usize, Error> {
        let name_ptr = self.read_user_usize(sp + 8)?;
        let edir_ptr = self.read_user_usize(sp + 16)?;
        let n_edir = self.read_user_usize(sp + 24)?;

        let name = self.strncpy_from_user(name_ptr, 1024)?;
        let glenda_stat = self.mgr.fs_client.stat_path(Badge::null(), &name)?;

        let p9_dir = serialize_p9_dir(&glenda_stat, &name);
        if p9_dir.len() > n_edir {
            return Err(Error::MessageTooLong);
        }
        self.copy_to_user(edir_ptr, &p9_dir)?;

        Ok(p9_dir.len())
    }

    pub fn sys_fstat(&mut self, sp: usize) -> Result<usize, Error> {
        let fd = self.read_user_usize(sp + 8)? as u32;
        let edir_ptr = self.read_user_usize(sp + 16)?;
        let n_edir = self.read_user_usize(sp + 24)?;

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let handle = task.files.get(fd).ok_or(Error::InvalidArgs)?;

        let glenda_stat = handle.fs_client.stat(Badge::null())?;
        let p9_dir = serialize_p9_dir(&glenda_stat, &handle.path);

        if p9_dir.len() > n_edir {
            return Err(Error::MessageTooLong);
        }
        self.copy_to_user(edir_ptr, &p9_dir)?;

        Ok(p9_dir.len())
    }
}

fn serialize_p9_dir(stat: &glenda::protocol::fs::Stat, name: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&[0, 0]); // size
    buf.extend_from_slice(&[0, 0]); // type
    buf.extend_from_slice(&0u32.to_le_bytes()); // dev
    buf.push(if stat.mode & 0o040000 != 0 { 0x80 } else { 0x00 }); // qid type
    buf.extend_from_slice(&0u32.to_le_bytes()); // qid vers
    buf.extend_from_slice(&(stat.ino as u64).to_le_bytes()); // qid path
    buf.extend_from_slice(&(stat.mode as u32).to_le_bytes());
    buf.extend_from_slice(&(stat.atime as u32).to_le_bytes());
    buf.extend_from_slice(&(stat.mtime as u32).to_le_bytes());
    buf.extend_from_slice(&(stat.size as u64).to_le_bytes());
    write_p9_string(&mut buf, name);
    write_p9_string(&mut buf, "glenda");
    write_p9_string(&mut buf, "glenda");
    write_p9_string(&mut buf, "glenda");

    let size = (buf.len() - 2) as u16;
    buf[0..2].copy_from_slice(&size.to_le_bytes());
    buf
}

fn write_p9_string(buf: &mut Vec<u8>, s: &str) {
    let len = s.len() as u16;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}
