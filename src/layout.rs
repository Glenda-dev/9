pub const UTZERO: usize = 0x10000;
pub const ARM64_ALIGN: usize = 0x10000; // 64KB
pub const USTKTOP: usize = 0x7_FFFF_0000;
pub const USTKSIZE: usize = 16 * 1024 * 1024; // 16MB

pub const VSPACE_SCRATCH_START: usize = 0x4000_0000;
pub const VSPACE_SCRATCH_END: usize = 0x8000_0000;
pub const CSPACE_DYNAMIC_L1_START_SLOT: usize = 0x1000;

pub const INIT_SLOT: usize = 0x10;
pub const FS_SLOT: usize = 0x11;
pub const VOLUME_SLOT: usize = 0x12;
pub const VT_SLOT: usize = 0x13;
pub const TIME_SLOT: usize = 0x14;
pub const AUTH_SLOT: usize = 0x15;

use glenda::cap::{CapPtr, Endpoint};

pub const INIT_CAP: Endpoint = Endpoint::from(CapPtr::from(INIT_SLOT));
pub const FS_CAP: Endpoint = Endpoint::from(CapPtr::from(FS_SLOT));
pub const VOLUME_CAP: Endpoint = Endpoint::from(CapPtr::from(VOLUME_SLOT));
pub const VT_CAP: Endpoint = Endpoint::from(CapPtr::from(VT_SLOT));
pub const TIME_CAP: Endpoint = Endpoint::from(CapPtr::from(TIME_SLOT));
pub const AUTH_CAP: Endpoint = Endpoint::from(CapPtr::from(AUTH_SLOT));
