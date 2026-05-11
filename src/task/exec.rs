use crate::nine::NineManager;
use crate::nine::mm::MemoryType;
use crate::aout::ExecHeader;
use crate::layout::{UTZERO, ARM64_ALIGN, USTKTOP, USTKSIZE};
use glenda::error::Error;
use glenda::mem::Perms;
use glenda::arch::mem::PGSIZE;
use glenda::utils::align::align_up;
use glenda::ipc::Badge;
use glenda::cap::{CapType, Page};
use glenda::interface::{ResourceService, VSpaceService, CSpaceService};
use alloc::vec::Vec;
use alloc::string::String;
use core::mem::size_of;

#[repr(C)]
#[derive(Default)]
struct Tos {
    prof_pp: usize,
    prof_next: usize,
    prof_last: usize,
    prof_first: usize,
    prof_pid: u32,
    prof_what: u32,
    cyclefreq: u64,
    kcycles: i64,
    pcycles: i64,
    pid: u32,
    clock: u32,
}

impl<'a> NineManager<'a> {
    pub fn exec_p9_binary(&mut self, pid: usize, data: &[u8], argv: &[String]) -> Result<(usize, usize, usize), Error> {
        let header = ExecHeader::parse(data)?;
        
        // 1. Calculate segment layouts
        let text_size = header.text as usize;
        let data_size = header.data as usize;
        let bss_size = header.bss as usize;
        
        let text_base = UTZERO;
        let text_end = text_base + 32 + text_size;
        let data_base = align_up(text_end, ARM64_ALIGN);
        let data_end = data_base + data_size;
        let bss_base = align_up(data_end, ARM64_ALIGN);
        
        log!("Nine: Loading a.out binary: text={:#x}@{:#x}, data={:#x}@{:#x}, bss={:#x}@{:#x}", 
            text_size, text_base, data_size, data_base, bss_size, bss_base);

        // 2. Map Text (ReadOnly + Exec)
        self.map_segment(pid, text_base, &data[0..(32 + text_size)], Perms::READ | Perms::EXECUTE, MemoryType::Text)?;
        
        // 3. Map Data (ReadWrite)
        if data_size > 0 {
            self.map_segment(pid, data_base, &data[(32 + text_size)..(32 + text_size + data_size)], Perms::READ | Perms::WRITE, MemoryType::Data)?;
        }
        
        // 4. Map BSS (ReadWrite, Zeroed)
        if bss_size > 0 {
            self.map_zero_segment(pid, bss_base, bss_size, Perms::READ | Perms::WRITE, MemoryType::Bss)?;
        }
        
        // 5. Setup Stack & Tos
        let initial_sp = self.setup_p9_stack(pid, argv)?;
        
        let entry = header.entry as usize;
        let tos_addr = USTKTOP - size_of::<Tos>();
        
        Ok((entry, initial_sp, tos_addr))
    }

    fn setup_p9_stack(&mut self, pid: usize, argv: &[String]) -> Result<usize, Error> {
        let stack_pages = USTKSIZE / PGSIZE;
        let stack_base = USTKTOP - USTKSIZE;
        
        // Map full stack (eager for now, can be lazy later)
        for i in 0..stack_pages {
            let vaddr = stack_base + i * PGSIZE;
            self.map_zero_segment(pid, vaddr, PGSIZE, Perms::READ | Perms::WRITE, MemoryType::Stack)?;
        }

        // Use scratch to build the initial stack content
        let last_page_vaddr = USTKTOP - PGSIZE;
        let task = self.task_registry.get(pid).ok_or(Error::NotFound)?;
        let map = task.mm.lookup_memory_map(last_page_vaddr).ok_or(Error::InvalidAddress)?;
        let frame = Page::from(glenda::cap::CapPtr::from(map.frame_cap));
        let scratch = self.vspace_mgr.map_scratch(frame, Perms::READ | Perms::WRITE, 1, &mut *self.res_client, &mut *self.cspace_mgr)?;
        
        let stack_buf = unsafe { core::slice::from_raw_parts_mut(scratch as *mut u8, PGSIZE) };
        stack_buf.fill(0);

        let mut top_offset = PGSIZE;
        
        // 1. Tos
        top_offset -= size_of::<Tos>();
        let tos = Tos {
            pid: pid as u32,
            ..Default::default()
        };
        unsafe {
            let tos_ptr = (scratch + top_offset) as *mut Tos;
            *tos_ptr = tos;
        }

        // 2. Arg strings
        let mut arg_vaddrs = Vec::new();
        for arg in argv.iter().rev() {
            let bytes = arg.as_bytes();
            top_offset -= bytes.len() + 1;
            stack_buf[top_offset..top_offset + bytes.len()].copy_from_slice(bytes);
            stack_buf[top_offset + bytes.len()] = 0;
            arg_vaddrs.push(USTKTOP - (PGSIZE - top_offset));
        }
        arg_vaddrs.reverse();

        // 3. Argv array
        top_offset -= (argv.len() + 1) * 8;
        top_offset &= !15; // Align
        for (i, vaddr) in arg_vaddrs.iter().enumerate() {
            let ptr = (scratch + top_offset + i * 8) as *mut usize;
            unsafe { *ptr = *vaddr };
        }

        // 4. argc
        top_offset -= 8;
        unsafe {
            let argc_ptr = (scratch + top_offset) as *mut usize;
            *argc_ptr = argv.len();
        }

        let final_sp = USTKTOP - (PGSIZE - top_offset);
        let _ = self.vspace_mgr.unmap(scratch, 1);
        
        Ok(final_sp)
    }

    fn map_segment(&mut self, _pid: usize, vaddr: usize, data: &[u8], perms: Perms, mem_type: MemoryType) -> Result<(), Error> {
        let size = data.len();
        let pages = align_up(size, PGSIZE) / PGSIZE;
        
        for i in 0..pages {
            let curr_vaddr = vaddr + i * PGSIZE;
            let frame_slot = self.cspace_mgr.alloc(&mut *self.res_client)?;
            self.res_client.alloc(Badge::null(), glenda::cap::CapType::Page, 1, frame_slot)?;
            let frame = Page::from(frame_slot);
            
            let task = self.task_registry.get(_pid).ok_or(Error::NotFound)?;
            task.vspace().map(frame, curr_vaddr, perms, 1)?;
            
            let scratch = self.vspace_mgr.map_scratch(frame, Perms::READ | Perms::WRITE, 1, &mut *self.res_client, &mut *self.cspace_mgr)?;
            let dst = unsafe { core::slice::from_raw_parts_mut(scratch as *mut u8, PGSIZE) };
            dst.fill(0);
            let start = i * PGSIZE;
            let end = core::cmp::min((i + 1) * PGSIZE, size);
            if start < size {
                dst[0..(end - start)].copy_from_slice(&data[start..end]);
            }
            let _ = self.vspace_mgr.unmap(scratch, 1);
            
            task.mm.add_memory_map(crate::nine::mm::MemoryMap {
                vaddr: curr_vaddr,
                size: PGSIZE,
                perms,
                mem_type,
                frame_cap: frame_slot.bits(),
            });
        }
        Ok(())
    }

    fn map_zero_segment(&mut self, pid: usize, vaddr: usize, size: usize, perms: Perms, mem_type: MemoryType) -> Result<(), Error> {
        let pages = align_up(size, PGSIZE) / PGSIZE;
        let task = self.task_registry.get(pid).ok_or(Error::NotFound)?;
        
        for i in 0..pages {
            let curr_vaddr = vaddr + i * PGSIZE;
            let frame_slot = self.cspace_mgr.alloc(&mut *self.res_client)?;
            self.res_client.alloc(Badge::null(), glenda::cap::CapType::Page, 1, frame_slot)?;
            let frame = Page::from(frame_slot);
            
            task.vspace().map(frame, curr_vaddr, perms, 1)?;
            
            task.mm.add_memory_map(crate::nine::mm::MemoryMap {
                vaddr: curr_vaddr,
                size: PGSIZE,
                perms,
                mem_type,
                frame_cap: frame_slot.bits(),
            });
        }
        Ok(())
    }
}
