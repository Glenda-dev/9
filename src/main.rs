#![no_std]
#![no_main]

#[macro_use]
extern crate glenda;
extern crate alloc;

mod aout;
mod arch;
mod config;
mod layout;
mod nine;
mod syscall;
mod task;

use alloc::boxed::Box;
use glenda::cap::{
    CSPACE_CAP, CapType, ENDPOINT_CAP, ENDPOINT_SLOT, MONITOR_CAP, RECV_SLOT, REPLY_SLOT, VSPACE_CAP,
};
use glenda::client::{
    AuthClient, FsClient, InitClient, ProcessClient, ResourceClient, TimeClient,
    VirtualTerminalClient, VolumeClient,
};
use glenda::ipc::Badge;
use glenda::protocol::resource::{
    FS_ENDPOINT, INIT_ENDPOINT, ResourceType, TIME_ENDPOINT, VOLUME_ENDPOINT, VT_ENDPOINT,
    FACTOTUM_ENDPOINT,
};
use glenda::runtime::{RuntimeThreadConfig, init_current_thread};
use glenda::interface::{CSpaceService, ResourceService, SystemService};
use glenda::utils::manager::{CSpaceManager, VSpaceManager};
use layout::*;
use nine::NineManager;

#[unsafe(no_mangle)]
fn main() -> usize {
    glenda::console::init_logging("Nine");
    log!("Starting Plan 9 / 9front Environment...");

    let cspace_mgr =
        Box::leak(Box::new(CSpaceManager::new(CSPACE_CAP, CSPACE_DYNAMIC_L1_START_SLOT)));
    let vspace_mgr = Box::leak(Box::new(VSpaceManager::new(
        VSPACE_CAP,
        VSPACE_SCRATCH_START,
        VSPACE_SCRATCH_END - VSPACE_SCRATCH_START,
    )));
    let res_client = Box::leak(Box::new(ResourceClient::new(MONITOR_CAP)));
    let proc_client = Box::leak(Box::new(ProcessClient::new(MONITOR_CAP)));

    // Request endpoints from resource monitor
    res_client
        .get_cap(Badge::null(), ResourceType::Endpoint, INIT_ENDPOINT, glenda::cap::CapPtr::from(INIT_SLOT))
        .expect("Failed to get init endpoint");
    let init_client = Box::leak(Box::new(InitClient::new(INIT_CAP)));

    res_client
        .get_cap(Badge::null(), ResourceType::Endpoint, FS_ENDPOINT, glenda::cap::CapPtr::from(FS_SLOT))
        .expect("Failed to get fs endpoint");
    let fs_client = Box::leak(Box::new(FsClient::new(FS_CAP)));

    res_client
        .get_cap(Badge::null(), ResourceType::Endpoint, VOLUME_ENDPOINT, glenda::cap::CapPtr::from(VOLUME_SLOT))
        .expect("Failed to get volume endpoint");
    let vol_client = Box::leak(Box::new(VolumeClient::new_simple(VOLUME_CAP, res_client)));

    res_client
        .get_cap(Badge::null(), ResourceType::Endpoint, VT_ENDPOINT, glenda::cap::CapPtr::from(VT_SLOT))
        .expect("Failed to get vt endpoint");
    let vt_client = Box::leak(Box::new(VirtualTerminalClient::new(VT_CAP)));

    res_client
        .get_cap(Badge::null(), ResourceType::Endpoint, TIME_ENDPOINT, glenda::cap::CapPtr::from(TIME_SLOT))
        .expect("Failed to get time endpoint");
    let time_client = Box::leak(Box::new(TimeClient::new(TIME_CAP)));

    res_client
        .get_cap(Badge::null(), ResourceType::Endpoint, FACTOTUM_ENDPOINT, glenda::cap::CapPtr::from(AUTH_SLOT))
        .expect("Failed to get factotum endpoint");
    let auth_client = Box::leak(Box::new(AuthClient::new(AUTH_CAP)));

    // Register Nine endpoint
    res_client
        .alloc(Badge::null(), CapType::Endpoint, 0, ENDPOINT_SLOT)
        .expect("Failed to alloc endpoint");

    // Register Nine endpoint to monitor
    res_client
        .register_cap(Badge::null(), ResourceType::Endpoint, glenda::protocol::resource::NINE_ENDPOINT, ENDPOINT_SLOT)
        .expect("Failed to register Nine endpoint");

    // Initialize runtime for this thread
    let main_park_slot =
        cspace_mgr.alloc(&mut *res_client).expect("Failed to alloc Nine main park slot");
    res_client
        .alloc(Badge::null(), CapType::Endpoint, 0, main_park_slot)
        .expect("Failed to alloc Nine main park endpoint");
    init_current_thread(RuntimeThreadConfig::new(glenda::cap::Endpoint::from(main_park_slot)))
        .expect("Failed to init Nine main thread runtime");

    let mut nine_mgr = NineManager::new(
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
    );

    nine_mgr.listen(ENDPOINT_CAP, REPLY_SLOT, RECV_SLOT).expect("Failed to listen");
    nine_mgr.init().expect("Failed to init");

    log!("Nine initialized and ready.");

    nine_mgr.run().expect("Nine main loop failed");

    0
}
