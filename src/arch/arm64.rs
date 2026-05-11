use glenda::ipc::MsgArgs;

pub mod constants {
    pub const INST_PAGE_FAULT: usize = 0x22; // Instruction Abort
    pub const LOAD_PAGE_FAULT: usize = 0x24; // Data Abort (Load)
    pub const STORE_PAGE_FAULT: usize = 0x25; // Data Abort (Store)
}

pub type SyscallArgs = [usize; 6];

/// Plan 9 arm64 ABI:
/// - x0: syscall number
/// - sp + 8: syscall args (on stack)
pub fn parse_syscall_args(args: MsgArgs) -> (usize, usize) {
    let sys_num = args[0];
    let sp = args[1]; // Assuming sp is at index 1 in fault message
    (sys_num, sp)
}
