use crate::layout::{USTKSIZE, USTKTOP};
use alloc::collections::BTreeMap;
use glenda::cap::{CapPtr, VSpace};
use glenda::mem::Perms;
use glenda::sync::rwlock::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    Text,
    Data,
    Bss,
    Stack,
    Heap,
    Anonymous,
}

#[derive(Debug, Clone)]
pub struct MemoryMap {
    pub vaddr: usize,
    pub size: usize,
    pub perms: Perms,
    pub mem_type: MemoryType,
    pub frame_cap: usize, // Bits of the frame capability
}

pub struct MmState {
    pub memory_maps: BTreeMap<usize, MemoryMap>,
    pub stack_top: usize,
    pub stack_size: usize,
    pub heap_start: usize,
    pub heap_brk: usize,
    pub intermediate_page_tables: BTreeMap<(usize, usize), CapPtr>,
}

pub struct MmStruct {
    pub vspace: VSpace,
    pub state: RwLock<MmState>,
}

impl MmStruct {
    pub fn new(vspace: VSpace) -> Self {
        Self {
            vspace,
            state: RwLock::new(MmState {
                memory_maps: BTreeMap::new(),
                stack_top: USTKTOP,
                stack_size: USTKSIZE,
                heap_start: 0, // Will be set after exec
                heap_brk: 0,
                intermediate_page_tables: BTreeMap::new(),
            }),
        }
    }

    pub fn lookup_memory_map(&self, vaddr: usize) -> Option<MemoryMap> {
        self.state
            .read()
            .memory_maps
            .range(..=vaddr)
            .next_back()
            .and_then(|(_, map)| (vaddr < map.vaddr + map.size).then_some(map.clone()))
    }

    pub fn add_memory_map(&self, map: MemoryMap) {
        self.state.write().memory_maps.insert(map.vaddr, map);
    }
}
