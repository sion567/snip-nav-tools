//! v1.8：复制并粘贴回原窗口（Raycast 式「Paste to frontmost app」）。
//!
//! 只在速查浮窗里生效：呼出浮窗的瞬间记下当时的 foreground 窗口（用户的
//! 终端/编辑器），Enter 复制后把浮窗藏起来、焦点自然回到原窗口，再补一次
//! Ctrl+V 模拟按键。用户自己的终端里仍是自己按回车执行——不代跑命令。
//! 默认关闭（settings: quick_paste = '1' 开启）。仅 Windows 实现，其他平台空转。

use std::sync::Mutex;

/// 呼出浮窗那一刻的前台窗口句柄（Windows HWND，仅本进程外才记录）。
pub struct PasteTarget(pub Mutex<Option<usize>>);

/// 记录当前前台窗口为目标（呼出浮窗时调用）。
#[allow(unused_variables)]
pub fn capture_target(state: &PasteTarget) {
    #[cfg(target_os = "windows")]
    {
        let hwnd = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        // 前台是自己的窗口（主窗/浮窗聚焦时按快捷键）→ 不记录，粘贴无意义
        let mut pid: u32 = 0;
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                hwnd,
                &mut pid,
            );
        }
        let ours = pid == std::process::id();
        let v = if hwnd != std::ptr::null_mut() && !ours {
            Some(hwnd as usize)
        } else {
            None
        };
        if let Ok(mut t) = state.0.lock() {
            *t = v;
        }
    }
}

/// 延迟后向目标窗口发送 Ctrl+V。
/// 调用方已把浮窗 hide——Windows 会把焦点还给前一个窗口，稍等片刻再补按键；
/// 若焦点没回去（罕见），用 AttachThreadInput 惯用手法尽力抢回前台。
#[allow(unused_variables)]
pub fn send_paste(target: Option<usize>) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(320));
        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

            if let Some(h) = target {
                let hwnd = h as *mut core::ffi::c_void;
                if IsWindow(hwnd) == 0 {
                    return;
                }
                let cur = GetForegroundWindow();
                if cur != hwnd {
                    let mut cur_pid: u32 = 0;
                    let mut tgt_pid: u32 = 0;
                    let cur_tid = GetWindowThreadProcessId(cur, &mut cur_pid);
                    let tgt_tid = GetWindowThreadProcessId(hwnd, &mut tgt_pid);
                    let my_tid = GetCurrentThreadId();
                    if tgt_tid != 0 && tgt_tid != my_tid {
                        AttachThreadInput(my_tid, tgt_tid, 1);
                        SetForegroundWindow(hwnd);
                        AttachThreadInput(my_tid, tgt_tid, 0);
                    }
                }
            }

            // Ctrl 按下 → V 按下 → V 抬起 → Ctrl 抬起
            let mut inputs: [INPUT; 4] = std::mem::zeroed();
            for inp in inputs.iter_mut() {
                inp.r#type = INPUT_KEYBOARD;
            }
            const VK_V: u16 = 0x56;
            inputs[0].Anonymous.ki = KEYBDINPUT {
                wVk: VK_CONTROL,
                wScan: 0,
                dwFlags: 0,
                time: 0,
                dwExtraInfo: 0,
            };
            inputs[1].Anonymous.ki = KEYBDINPUT {
                wVk: VK_V,
                wScan: 0,
                dwFlags: 0,
                time: 0,
                dwExtraInfo: 0,
            };
            inputs[2].Anonymous.ki = KEYBDINPUT {
                wVk: VK_V,
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };
            inputs[3].Anonymous.ki = KEYBDINPUT {
                wVk: VK_CONTROL,
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    });
}
