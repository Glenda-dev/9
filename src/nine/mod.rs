use glenda::cap::{CapPtr, Endpoint, Reply, TCB, TCB_SLOT, VSPACE_SLOT};
use glenda::client::{
    AuthClient, FsClient, InitClient, ProcessClient, ResourceClient, TimeClient,
    VirtualTerminalClient, VolumeClient,
};
use glenda::error::Error;
use glenda::interface::{
    CSpaceService, FaultService, FileHandleService, FileSystemService, InitService, ProcessService,
    SystemService, VirtualFileSystemService, VolumeService,
};
use glenda::ipc::{Badge, MsgArgs, UTCB};
use glenda::protocol;
use glenda::utils::manager::{CSpaceManager, VSpaceManager};
use crate::config::NineConfig;

pub mod mm;
pub mod task;
pub mod user;

pub struct NineIpc {
    pub running: bool,
    pub endpoint: Endpoint,
    pub reply: Reply,
    pub recv: CapPtr,
}

pub struct NineManager<'a> {
    pub ipc: NineIpc,
    pub task_registry: task::TaskRegistry,
    pub config: NineConfig,
    pub init_client: &'a mut InitClient,
    pub proc_client: &'a mut ProcessClient,
    pub res_client: &'a mut ResourceClient,
    pub vt_client: &'a mut VirtualTerminalClient,
    pub vol_client: &'a mut VolumeClient,
    pub fs_client: &'a mut FsClient,
    pub time_client: &'a mut TimeClient,
    pub auth_client: &'a mut AuthClient,
    pub cspace_mgr: &'a mut CSpaceManager,
    pub vspace_mgr: &'a mut VSpaceManager,
}

impl<'a> NineManager<'a> {
    pub fn new(
        init_client: &'a mut InitClient,
        proc_client: &'a mut ProcessClient,
        res_client: &'a mut ResourceClient,
        vt_client: &'a mut VirtualTerminalClient,
        vol_client: &'a mut VolumeClient,
        fs_client: &'a mut FsClient,
        time_client: &'a mut TimeClient,
        auth_client: &'a mut AuthClient,
        cspace_mgr: &'a mut CSpaceManager,
        vspace_mgr: &'a mut VSpaceManager,
    ) -> Self {
        Self {
            ipc: NineIpc {
                running: false,
                endpoint: Endpoint::from(CapPtr::null()),
                reply: Reply::from(CapPtr::null()),
                recv: CapPtr::null(),
            },
            task_registry: task::TaskRegistry::new(),
            config: NineConfig::default(),
            init_client,
            proc_client,
            res_client,
            vt_client,
            vol_client,
            fs_client,
            time_client,
            auth_client,
            cspace_mgr,
            vspace_mgr,
        }
    }

    pub fn bootstrap(&mut self) -> Result<(), Error> {
        self.init_client.report_service(Badge::null(), protocol::init::ServiceState::Starting)?;

        if let Ok(config) = NineConfig::load(self.res_client, self.cspace_mgr, self.vspace_mgr) {
            log!("Nine: Loaded config, init_path={}", config.init_path);
            self.config = config;
        }

        log!("Nine: Mounting rootfs partition {} -> /", self.config.root_partition);
        let rootfs_slot = self.cspace_mgr.alloc(&mut *self.res_client)?;
        let target_ep = self.vol_client.mount_partition(
            Badge::null(),
            &self.config.root_partition,
            rootfs_slot,
        )?;
        self.fs_client.mount(Badge::null(), "/", target_ep)?;

        let view_id = self.fs_client.create_view(Badge::null(), "/")?;
        self.fs_client.set_view(Badge::null(), view_id)?;

        self.load_init()?;

        Ok(())
    }

    fn load_init(&mut self) -> Result<(), Error> {
        log!("Nine: Loading init process: {}", self.config.init_path);

        let host_pid = self.proc_client.create(Badge::null(), "nine_init")?;

        let cnode_slot = self.cspace_mgr.alloc(&mut *self.res_client)?;
        let _cnode = self.proc_client.get_cnode(Badge::null(), host_pid, cnode_slot)?;

        let task = alloc::sync::Arc::new(task::Task::new(host_pid, cnode_slot));
        self.task_registry.register(task.clone());

        let fd_slot = self.cspace_mgr.alloc(&mut *self.res_client)?;
        let _ = self.fs_client.open(
            Badge::null(),
            &self.config.init_path,
            glenda::protocol::fs::OpenFlags::O_RDONLY,
            0,
            fd_slot,
        )?;
        let mut handle = FsClient::new(Endpoint::from(fd_slot));

        let stat = handle.stat(Badge::null())?;
        let mut binary = alloc::vec::Vec::with_capacity(stat.size as usize);
        let mut offset = 0;
        while offset < stat.size as usize {
            let mut buf = [0u8; 4096];
            let n = handle.read(Badge::null(), offset, &mut buf)?;
            if n == 0 {
                break;
            }
            binary.extend_from_slice(&buf[..n]);
            offset += n;
        }
        let _ = handle.close(Badge::null());

        let (entry, sp, tos_addr) =
            self.exec_p9_binary(host_pid, &binary, &[self.config.init_path.clone()])?;

        let tcb = TCB::from(CapPtr::concat(cnode_slot, TCB_SLOT));
        tcb.set_entrypoint(entry, sp, tos_addr)?;
        let fault_ep = self.ipc.endpoint;
        tcb.set_fault_handler(fault_ep)?;

        tcb.resume()?;
        log!("Nine: Resumed init process at {:#x}, sp={:#x}, tos={:#x}", entry, sp, tos_addr);

        Ok(())
    }

    pub fn handle_syscall_abi(&mut self, pid: usize, args: MsgArgs) -> Result<(), Error> {
        let (sys_num, sp) = crate::arch::parse_syscall_args(args);
        let syscall = crate::syscall::Plan9Syscall::from(sys_num);
        debug!("Nine: Syscall from pid {}: {:?} (num={}), sp={:#x}", pid, syscall, sys_num, sp);

        let ret = self.with_user_session(pid, |sess| {
            match syscall {
                crate::syscall::Plan9Syscall::Open => sess.sys_open(sp),
                crate::syscall::Plan9Syscall::Read => sess.sys_read(sp),
                crate::syscall::Plan9Syscall::Write => sess.sys_write(sp),
                crate::syscall::Plan9Syscall::Pread => sess.sys_pread(sp),
                crate::syscall::Plan9Syscall::Pwrite => sess.sys_pwrite(sp),
                crate::syscall::Plan9Syscall::Seek => sess.sys_seek(sp),
                crate::syscall::Plan9Syscall::Close => sess.sys_close(sp),
                crate::syscall::Plan9Syscall::Brk => sess.sys_brk(sp),
                crate::syscall::Plan9Syscall::Rfork => sess.sys_rfork(sp),
                crate::syscall::Plan9Syscall::Stat => sess.sys_stat(sp),
                crate::syscall::Plan9Syscall::Fstat => sess.sys_fstat(sp),
                crate::syscall::Plan9Syscall::Fd2path => sess.sys_fd2path(sp),
                crate::syscall::Plan9Syscall::Bind => sess.sys_bind(sp),
                crate::syscall::Plan9Syscall::Mount => sess.sys_mount(sp),
                crate::syscall::Plan9Syscall::Exits => {
                    let msg_ptr = sess.read_user_usize(sp + 8)?;
                    if msg_ptr != 0 {
                        let msg = sess.strncpy_from_user(msg_ptr, 128)?;
                        log!("Nine: pid {} exiting with msg: {}", pid, msg);
                    } else {
                        log!("Nine: pid {} exiting", pid);
                    }
                    Ok(0)
                }
                _ => {
                    warn!("Nine: Unimplemented syscall: {:?}", syscall);
                    Err(Error::NotSupported)
                }
            }
        });

        let mut utcb = unsafe { UTCB::new() };
        utcb.clear();
        match ret {
            Ok(v) => utcb.set_mr(0, v),
            Err(e) => {
                error!("Nine: Syscall {:?} failed: {:?}", syscall, e);
                utcb.set_mr(0, usize::MAX);
            }
        }
        Ok(())
    }
}

impl<'a> SystemService for NineManager<'a> {
    fn init(&mut self) -> Result<(), Error> {
        self.bootstrap()
    }

    fn listen(&mut self, ep: Endpoint, reply: CapPtr, recv: CapPtr) -> Result<(), Error> {
        self.ipc.endpoint = ep;
        self.ipc.reply = Reply::from(reply);
        self.ipc.recv = recv;
        Ok(())
    }

    fn run(&mut self) -> Result<(), Error> {
        self.ipc.running = true;
        self.init_client.report_service(Badge::null(), protocol::init::ServiceState::Running)?;

        while self.ipc.running {
            let mut utcb = unsafe { UTCB::new() };
            utcb.clear();
            utcb.set_reply_window(self.ipc.reply.cap());
            utcb.set_recv_window(self.ipc.recv);

            if let Err(e) = self.ipc.endpoint.recv(&mut utcb) {
                error!("Nine: Recv error: {:?}", e);
                continue;
            }

            let _should_reply = match self.dispatch(&mut utcb) {
                Ok(()) => true,
                Err(Error::Success) => false,
                Err(e) => {
                    error!("Nine: Dispatch error: {:?}", e);
                    false
                }
            };

            if _should_reply {
                let _ = self.ipc.reply.reply(&mut utcb);
            }
        }
        Ok(())
    }

    fn dispatch(&mut self, utcb: &mut UTCB) -> Result<(), Error> {
        let badge = utcb.get_badge();

        glenda::ipc_dispatch! {
            self, utcb,
            (protocol::KERNEL_PROTO, protocol::kernel::SYSCALL) => |s: &mut NineManager, utcb: &mut UTCB| {
                let mut args = [0usize; 8];
                for i in 0..8 {
                    args[i] = utcb.get_mr(i);
                }
                s.handle_syscall_abi(badge.bits(), args)
            },
            (protocol::KERNEL_PROTO, protocol::kernel::PAGE_FAULT) => |s: &mut NineManager, utcb: &mut UTCB| s.page_fault(badge, utcb.get_mr(0), utcb.get_mr(1), utcb.get_mr(2)),
            (protocol::KERNEL_PROTO, protocol::kernel::ILLEGAL_INSTRUCTION) => |s: &mut NineManager, utcb: &mut UTCB| s.illegal_instruction(badge, utcb.get_mr(0), utcb.get_mr(1)),
            (protocol::KERNEL_PROTO, protocol::kernel::BREAKPOINT) => |s: &mut NineManager, utcb: &mut UTCB| s.breakpoint(badge, utcb.get_mr(0)),
            (protocol::KERNEL_PROTO, protocol::kernel::ACCESS_FAULT) => |s: &mut NineManager, utcb: &mut UTCB| s.access_fault(badge, utcb.get_mr(0), utcb.get_mr(1)),
            (protocol::KERNEL_PROTO, protocol::kernel::ACCESS_MISALIGNED) => |s: &mut NineManager, utcb: &mut UTCB| s.access_misaligned(badge, utcb.get_mr(0), utcb.get_mr(1)),
            (protocol::KERNEL_PROTO, protocol::kernel::VIRT_EXIT) => |s: &mut NineManager, utcb: &mut UTCB| s.virt_exit(badge, utcb.get_mr(0), utcb.get_mr(1), utcb.get_mr(2), utcb.get_mr(3)),
            (protocol::KERNEL_PROTO, protocol::kernel::UNKNOWN_FAULT) => |s: &mut NineManager, utcb: &mut UTCB| s.unknown_fault(badge, utcb.get_mr(0), utcb.get_mr(1), utcb.get_mr(2)),
            (_, _) => |_: &mut NineManager, _: &mut UTCB| Err(Error::InvalidProtocol),
        }
    }

    fn reply(&mut self, utcb: &mut UTCB) -> Result<(), Error> {
        self.ipc.reply.reply(utcb)
    }

    fn stop(&mut self) {
        self.ipc.running = false;
    }
}

impl<'a> FaultService for NineManager<'a> {
    fn page_fault(
        &mut self,
        badge: Badge,
        addr: usize,
        pc: usize,
        cause: usize,
    ) -> Result<(), Error> {
        debug!(
            "Nine: Page fault pid={} addr={:#x} pc={:#x} cause={:#x}",
            badge.bits(),
            addr,
            pc,
            cause
        );
        // TODO: Handle lazy mapping
        Ok(())
    }

    fn illegal_instruction(&mut self, badge: Badge, inst: usize, pc: usize) -> Result<(), Error> {
        error!("Nine: Illegal instruction pid={} inst={:#x} pc={:#x}", badge.bits(), inst, pc);
        Err(Error::NotSupported)
    }

    fn breakpoint(&mut self, badge: Badge, pc: usize) -> Result<(), Error> {
        warn!("Nine: Breakpoint pid={} pc={:#x}", badge.bits(), pc);
        Ok(())
    }

    fn access_fault(&mut self, badge: Badge, addr: usize, pc: usize) -> Result<(), Error> {
        error!("Nine: Access fault pid={} addr={:#x} pc={:#x}", badge.bits(), addr, pc);
        Err(Error::PermissionDenied)
    }

    fn access_misaligned(&mut self, badge: Badge, addr: usize, pc: usize) -> Result<(), Error> {
        error!("Nine: Access misaligned pid={} addr={:#x} pc={:#x}", badge.bits(), addr, pc);
        Err(Error::InvalidArgs)
    }

    fn virt_exit(
        &mut self,
        badge: Badge,
        reason: usize,
        d0: usize,
        d1: usize,
        d2: usize,
    ) -> Result<(), Error> {
        error!(
            "Nine: Virt exit pid={} reason={:#x} details: {:#x} {:#x} {:#x}",
            badge.bits(),
            reason,
            d0,
            d1,
            d2
        );
        Err(Error::NotSupported)
    }

    fn unknown_fault(
        &mut self,
        badge: Badge,
        cause: usize,
        value: usize,
        pc: usize,
    ) -> Result<(), Error> {
        error!(
            "Nine: Unknown fault pid={} cause={:#x} value={:#x} pc={:#x}",
            badge.bits(),
            cause,
            value,
            pc
        );
        Err(Error::NotSupported)
    }

    fn handle_syscall(&mut self, pid: usize, args: MsgArgs) -> Result<(), Error> {
        self.handle_syscall_abi(pid, args)
    }
}
