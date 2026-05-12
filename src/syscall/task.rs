use crate::nine::user::UserAccessSession;
use glenda::arch::mem::PGSIZE;
use glenda::cap::{CapType, Page};
use glenda::error::Error;
use glenda::interface::{CSpaceService, ResourceService, VSpaceService};
use glenda::ipc::Badge;
use glenda::mem::Perms;
use glenda::utils::align::align_up;

impl<'a, 'b> UserAccessSession<'a, 'b> {
    pub fn sys_brk(&mut self, sp: usize) -> Result<usize, Error> {
        let addr = self.read_user_usize(sp + 8)?;
        debug!("Nine: sys_brk({:#x})", addr);

        let task = self.mgr.task_registry.get(self.pid).ok_or(Error::NotFound)?;
        let mut mm = task.mm.state.write();

        if addr == 0 {
            return Ok(mm.heap_brk);
        }

        if addr < mm.heap_start {
            return Err(Error::InvalidAddress);
        }

        let new_brk = align_up(addr, PGSIZE);
        let old_brk = mm.heap_brk;

        if new_brk > old_brk {
            let pages = (new_brk - old_brk) / PGSIZE;
            for i in 0..pages {
                let curr_vaddr = old_brk + i * PGSIZE;
                let frame_slot = self.mgr.cspace_mgr.alloc(&mut *self.mgr.res_client)?;
                self.mgr.res_client.alloc(Badge::null(), CapType::Page, 1, frame_slot)?;
                let frame = Page::from(frame_slot);

                task.vspace().map(frame, curr_vaddr, Perms::READ | Perms::WRITE, 1)?;

                mm.memory_maps.insert(
                    curr_vaddr,
                    crate::nine::mm::MemoryMap {
                        vaddr: curr_vaddr,
                        size: PGSIZE,
                        perms: Perms::READ | Perms::WRITE,
                        mem_type: crate::nine::mm::MemoryType::Heap,
                        frame_cap: frame_slot.bits(),
                    },
                );
            }
        } else if new_brk < old_brk {
            // TODO: Unmap pages if shrinking
        }

        mm.heap_brk = addr;
        Ok(0)
    }

    pub fn sys_rfork(&mut self, sp: usize) -> Result<usize, Error> {
        let flags = self.read_user_usize(sp + 8)?;
        debug!("Nine: sys_rfork({:#x})", flags);

        // RFPROC = 0x00000010
        // RFMEM  = 0x00000001
        // RFFDG  = 0x00000002
        // RFNOTEG= 0x00000004
        // RFNAMEG= 0x00000008
        // RFENVG = 0x00000020

        if flags & 0x10 != 0 {
            // RFPROC: Fork process
            warn!("Nine: rfork(RFPROC) not fully implemented");
            // 1. Create new host process via Warren
            // 2. Duplicate state
            // 3. Return 0 in child, child PID in parent
            return Err(Error::NotSupported);
        }

        Ok(0)
    }
}
