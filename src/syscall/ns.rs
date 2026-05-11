use crate::nine::user::UserAccessSession;
use glenda::error::Error;
use glenda::ipc::Badge;
use glenda::interface::{FileSystemService, VirtualFileSystemService, CSpaceService};
use glenda::cap::Endpoint;
use alloc::string::String;

impl<'a, 'b> UserAccessSession<'a, 'b> {
    pub fn sys_bind(&mut self, sp: usize) -> Result<usize, Error> {
        let name_ptr = self.read_user_usize(sp + 8)?;
        let old_ptr = self.read_user_usize(sp + 16)?;
        let flags = self.read_user_usize(sp + 24)?;
        
        let name = self.strncpy_from_user(name_ptr, 1024)?;
        let old = self.strncpy_from_user(old_ptr, 1024)?;
        
        debug!("Nine: sys_bind(\"{}\", \"{}\", {:#x})", name, old, flags);
        
        let fd_slot = self.mgr.cspace_mgr.alloc(&mut *self.mgr.res_client)?;
        let _ = self.mgr.fs_client.open(Badge::null(), &name, glenda::protocol::fs::OpenFlags::O_RDONLY, 0, fd_slot)?;
        
        let source_ep = Endpoint::from(fd_slot);
        self.mgr.fs_client.mount(Badge::null(), &old, source_ep)?;
        
        Ok(0)
    }

    pub fn sys_mount(&mut self, sp: usize) -> Result<usize, Error> {
        let fd = self.read_user_usize(sp + 8)? as u32;
        let afd = self.read_user_usize(sp + 16)? as i32;
        let old_ptr = self.read_user_usize(sp + 24)?;
        let flags = self.read_user_usize(sp + 32)?;
        let aname_ptr = self.read_user_usize(sp + 40)?;
        
        let old = self.strncpy_from_user(old_ptr, 1024)?;
        let aname = if aname_ptr != 0 { self.strncpy_from_user(aname_ptr, 1024)? } else { String::from("") };
        
        debug!("Nine: sys_mount(fd={}, afd={}, \"{}\", {:#x}, \"{}\")", fd, afd, old, flags, aname);
        
        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let handle = task.files.get(fd).ok_or(Error::InvalidArgs)?;
        
        self.mgr.fs_client.mount(Badge::null(), &old, handle.fs_client.endpoint())?;
        
        Ok(0)
    }

    pub fn sys_unmount(&mut self, sp: usize) -> Result<usize, Error> {
        let _name_ptr = self.read_user_usize(sp + 8)?;
        let old_ptr = self.read_user_usize(sp + 16)?;
        
        let old = if old_ptr != 0 { self.strncpy_from_user(old_ptr, 1024)? } else { String::from("") };
        
        debug!("Nine: sys_unmount(\"{}\")", old);
        self.mgr.fs_client.unmount(Badge::null(), &old)?;
        
        Ok(0)
    }
}
