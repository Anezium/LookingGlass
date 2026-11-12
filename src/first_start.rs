//! First-start checks on the Vita, run before the game opens its assets.
//!
//! Without them a missing data folder ends the app with `exit(1)` before any window, and the
//! error line goes only to the kernel debug output. Here the player gets the system message
//! dialog (SceCommonDialog), which the system draws itself: it needs vitaGL's display setup
//! but no shader of ours, so it also works when the shader compiler is absent.
use std::ffi::CString;
use std::path::Path;

#[repr(C)]
struct CommonDialogParam {
    infobar_param: *mut core::ffi::c_void,
    bg_color: *mut core::ffi::c_void,
    dimmer_color: *mut core::ffi::c_void,
    reserved: [u8; 60],
    magic: u32,
}

#[repr(C)]
struct MsgDialogUserMessageParam {
    button_type: i32,
    msg: *const u8,
    button_param: *mut core::ffi::c_void,
    reserved: [u8; 28],
}

#[repr(C)]
struct MsgDialogParam {
    sdk_version: u32,
    common: CommonDialogParam,
    mode: i32,
    user_msg_param: *mut MsgDialogUserMessageParam,
    sys_msg_param: *mut core::ffi::c_void,
    error_code_param: *mut core::ffi::c_void,
    prog_bar_param: *mut core::ffi::c_void,
    flag: i32,
    reserved: [u8; 32],
}

// Sizes from the VITASDK_BUILD_ASSERT_EQ lines in psp2/common_dialog.h and psp2/message_dialog.h.
const _: () = assert!(core::mem::size_of::<CommonDialogParam>() == 0x4C);
const _: () = assert!(core::mem::size_of::<MsgDialogUserMessageParam>() == 0x28);
const _: () = assert!(core::mem::size_of::<MsgDialogParam>() == 0x88);

const PSP2_SDK_VERSION: u32 = 0x0357_0011;
const COMMON_DIALOG_MAGIC_NUMBER: u32 = 0xC0D1_A109;
const MSG_DIALOG_MODE_USER_MSG: i32 = 1;
const MSG_DIALOG_BUTTON_TYPE_OK: i32 = 0;
const COMMON_DIALOG_STATUS_RUNNING: i32 = 1;
const MSG_DIALOG_USER_MSG_SIZE: usize = 512;
const GL_COLOR_BUFFER_BIT: u32 = 0x0000_4000;
const SCE_GXM_MULTISAMPLE_NONE: i32 = 0;

// libvitaGL and the SceCommonDialog stub are already on the link line through miniquad.
// build.rs wraps vglSwapBuffers for frame timing; this path calls the real one.
extern "C" {
    fn vglInitExtended(pool_size: i32, width: i32, height: i32, ram_threshold: i32, msaa: i32) -> u8;
    fn __real_vglSwapBuffers(has_commondialog: u8);
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glClear(mask: u32);
    fn sceMsgDialogInit(param: *const MsgDialogParam) -> i32;
    fn sceMsgDialogGetStatus() -> i32;
    fn sceMsgDialogTerm() -> i32;
}

/// Checks the data folder. On failure, shows the dialog and ends the process when the
/// player closes it; on success, returns and the normal start continues.
pub fn check_or_exit(base: &Path) {
    let problems = crate::data_check::check(base, &crate::data_check::REQUIRED_PACKS);
    if problems.is_empty() {
        return;
    }
    let text = crate::data_check::message(base, &problems);
    eprintln!("[lg/first-start] {}", text.replace('\n', " | "));
    show_and_exit(&text);
}

fn show_and_exit(text: &str) -> ! {
    let mut text = text.replace('\0', "");
    if text.len() >= MSG_DIALOG_USER_MSG_SIZE {
        let mut end = MSG_DIALOG_USER_MSG_SIZE - 1;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    let text = CString::new(text).expect("NUL bytes were removed");
    unsafe {
        // The process ends below, so the pool sizes only need to leave the dialog its memory:
        // vglInitExtended keeps the common dialog budget out of vitaGL's pools.
        vglInitExtended(0, 960, 544, 16 * 1024 * 1024, SCE_GXM_MULTISAMPLE_NONE);
        let mut user = MsgDialogUserMessageParam {
            button_type: MSG_DIALOG_BUTTON_TYPE_OK,
            msg: text.as_ptr().cast(),
            button_param: core::ptr::null_mut(),
            reserved: [0; 28],
        };
        // Boxed so the address the magic number is derived from stays fixed.
        let mut param = Box::new(MsgDialogParam {
            sdk_version: PSP2_SDK_VERSION,
            common: CommonDialogParam {
                infobar_param: core::ptr::null_mut(),
                bg_color: core::ptr::null_mut(),
                dimmer_color: core::ptr::null_mut(),
                reserved: [0; 60],
                magic: 0,
            },
            mode: MSG_DIALOG_MODE_USER_MSG,
            user_msg_param: &mut user,
            sys_msg_param: core::ptr::null_mut(),
            error_code_param: core::ptr::null_mut(),
            prog_bar_param: core::ptr::null_mut(),
            flag: 0,
            reserved: [0; 32],
        });
        // _sceCommonDialogSetMagicNumber: magic number plus the address of commonParam.
        param.common.magic = COMMON_DIALOG_MAGIC_NUMBER.wrapping_add(&param.common as *const _ as usize as u32);
        let result = sceMsgDialogInit(&*param);
        eprintln!("[lg/first-start] sceMsgDialogInit={result:#x}");
        if result >= 0 {
            loop {
                glClearColor(0.0, 0.0, 0.0, 1.0);
                glClear(GL_COLOR_BUFFER_BIT);
                __real_vglSwapBuffers(1);
                if sceMsgDialogGetStatus() != COMMON_DIALOG_STATUS_RUNNING {
                    break;
                }
            }
            sceMsgDialogTerm();
        }
    }
    std::process::exit(1);
}
