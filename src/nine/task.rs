use crate::nine::mm::MmStruct;
use alloc::sync::Arc;
use glenda::cap::{CapPtr, VSpace};
use glenda::client::FsClient;
use alloc::collections::BTreeMap;
use glenda::sync::rwlock::RwLock;
use alloc::string::String;

#[derive(Debug, Clone)]
pub struct FileHandle {
    pub fs_client: FsClient,
    pub fs_ep_slot: CapPtr,
    pub offset: usize,
    pub path: String,
}

pub struct FilesState {
    pub fds: BTreeMap<u32, FileHandle>,
    pub next_fd: u32,
}

pub struct FilesStruct {
    pub state: RwLock<FilesState>,
}

impl FilesStruct {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(FilesState {
                fds: BTreeMap::new(),
                next_fd: 0,
            }),
        }
    }

    pub fn open(&self, fs_client: FsClient, fs_ep_slot: CapPtr, path: String) -> u32 {
        let mut state = self.state.write();
        let fd = state.next_fd;
        state.fds.insert(fd, FileHandle {
            fs_client,
            fs_ep_slot,
            offset: 0,
            path,
        });
        state.next_fd += 1;
        fd
    }

    pub fn get(&self, fd: u32) -> Option<FileHandle> {
        self.state.read().fds.get(&fd).cloned()
    }

    pub fn close(&self, fd: u32) {
        self.state.write().fds.remove(&fd);
    }
}

pub struct Task {
    pub pid: usize,
    pub vspace_cap: CapPtr,
    pub mm: MmStruct,
    pub files: FilesStruct,
}

impl Task {
    pub fn new(pid: usize, vspace_cap: CapPtr) -> Self {
        Self {
            pid,
            vspace_cap,
            mm: MmStruct::new(VSpace::from(vspace_cap)),
            files: FilesStruct::new(),
        }
    }

    pub fn vspace(&self) -> VSpace {
        VSpace::from(self.vspace_cap)
    }
}

pub struct TaskRegistry {
    pub tasks: BTreeMap<usize, Arc<Task>>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, task: Arc<Task>) {
        self.tasks.insert(task.pid, task);
    }

    pub fn get(&self, pid: usize) -> Option<Arc<Task>> {
        self.tasks.get(&pid).cloned()
    }

    pub fn remove(&mut self, pid: usize) {
        self.tasks.remove(&pid);
    }
}
