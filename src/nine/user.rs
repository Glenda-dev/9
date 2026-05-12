use crate::nine::NineManager;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::min;
use core::mem::size_of;
use glenda::error::Error;
use glenda::interface::VSpaceService;
use glenda::mem::Perms;

pub struct UserAccessSession<'a, 'b> {
    pub mgr: &'a mut NineManager<'b>,
    pub pid: usize,
}

impl<'a, 'b> UserAccessSession<'a, 'b> {
    pub fn new(mgr: &'a mut NineManager<'b>, pid: usize) -> Self {
        Self { mgr, pid }
    }

    pub fn read_user_usize(&mut self, user_addr: usize) -> Result<usize, Error> {
        let mut buf = [0u8; size_of::<usize>()];
        self.copy_from_user(user_addr, &mut buf)?;
        Ok(usize::from_ne_bytes(buf))
    }

    pub fn copy_from_user(&mut self, user_src: usize, dst: &mut [u8]) -> Result<(), Error> {
        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let mut copied = 0;
        while copied < dst.len() {
            let cursor = user_src + copied;
            let map = task.mm.lookup_memory_map(cursor).ok_or(Error::InvalidAddress)?;
            if !map.perms.contains(Perms::READ) {
                return Err(Error::PermissionDenied);
            }
            let offset = cursor - map.vaddr;
            let chunk = min(map.size - offset, dst.len() - copied);

            // Map scratch and copy
            let frame = glenda::cap::Page::from(glenda::cap::CapPtr::from(map.frame_cap));
            let scratch = self.mgr.vspace_mgr.map_scratch(
                frame,
                Perms::READ,
                1,
                &mut *self.mgr.res_client,
                &mut *self.mgr.cspace_mgr,
            )?;

            let src_slice = unsafe {
                core::slice::from_raw_parts((scratch + (offset % 4096)) as *const u8, chunk)
            };
            dst[copied..copied + chunk].copy_from_slice(src_slice);

            let _ = self.mgr.vspace_mgr.unmap(scratch, 1);
            copied += chunk;
        }
        Ok(())
    }

    pub fn copy_to_user(&mut self, user_dst: usize, src: &[u8]) -> Result<(), Error> {
        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let mut copied = 0;

        while copied < src.len() {
            let cursor = user_dst + copied;
            let map = task.mm.lookup_memory_map(cursor).ok_or(Error::InvalidAddress)?;
            if !map.perms.contains(Perms::WRITE) {
                return Err(Error::PermissionDenied);
            }
            let offset = cursor - map.vaddr;
            let chunk = min(map.size - offset, src.len() - copied);

            let frame = glenda::cap::Page::from(glenda::cap::CapPtr::from(map.frame_cap));
            let scratch = self.mgr.vspace_mgr.map_scratch(
                frame,
                Perms::READ | Perms::WRITE,
                1,
                &mut *self.mgr.res_client,
                &mut *self.mgr.cspace_mgr,
            )?;

            let dst_slice = unsafe {
                core::slice::from_raw_parts_mut((scratch + (offset % 4096)) as *mut u8, chunk)
            };
            dst_slice.copy_from_slice(&src[copied..copied + chunk]);

            let _ = self.mgr.vspace_mgr.unmap(scratch, 1);
            copied += chunk;
        }
        Ok(())
    }

    pub fn strncpy_from_user(&mut self, user_src: usize, max_len: usize) -> Result<String, Error> {
        let mut out = Vec::new();
        let mut buf = [0u8; 1];
        for i in 0..max_len {
            self.copy_from_user(user_src + i, &mut buf)?;
            if buf[0] == 0 {
                break;
            }
            out.push(buf[0]);
        }
        String::from_utf8(out).map_err(|_| Error::InvalidArgs)
    }
}

impl<'a> NineManager<'a> {
    pub fn with_user_session<T, F>(&mut self, pid: usize, f: F) -> Result<T, Error>
    where
        F: FnOnce(&mut UserAccessSession<'_, 'a>) -> Result<T, Error>,
    {
        let mut sess = UserAccessSession::new(self, pid);
        f(&mut sess)
    }
}
