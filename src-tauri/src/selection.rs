//! Reading the text the user has selected in another app.
//!
//! There's no cross-platform API for "the selection", so Tududu does what the
//! user would: presses Cmd+C / Ctrl+C and reads the clipboard. The clipboard's
//! change counter tells whether the copy happened, so with nothing selected
//! the text already on the clipboard isn't mistaken for the selection.

use crate::input::{self, EnigoState};
use log::{debug, warn};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

/// How long to wait for the user to let go of the shortcut's modifier keys.
const MODIFIER_RELEASE_TIMEOUT: Duration = Duration::from_millis(1500);
/// How long the focused app gets to put its selection on the clipboard.
const COPY_TIMEOUT: Duration = Duration::from_millis(800);

/// What [`copy_selected_text`] found.
pub struct CopiedSelection {
    /// The selected text, or `None` when nothing was selected.
    pub text: Option<String>,
    /// The clipboard text from before the copy, to put back if the selection
    /// isn't used. `None` when the clipboard held no text.
    pub previous_clipboard: Option<String>,
}

/// Copy the focused app's selection. Blocks for up to ~2 s; call it off the
/// main thread.
pub fn copy_selected_text(app: &AppHandle) -> Result<CopiedSelection, String> {
    // Still-held Option/Shift would turn Cmd+C into Cmd+Option+Shift+C.
    if !wait_for_modifiers_released(MODIFIER_RELEASE_TIMEOUT) {
        warn!("Modifier keys still held; copying anyway");
    }

    let previous_clipboard = app.clipboard().read_text().ok();
    let before = clipboard_change_count();

    // macOS resolves the C key for the keyboard layout via TIS, which must run
    // on the main thread.
    let (tx, rx) = std::sync::mpsc::channel();
    let main_app = app.clone();
    app.run_on_main_thread(move || {
        let result = main_app
            .try_state::<EnigoState>()
            .ok_or_else(|| "Enigo state not initialized".to_string())
            .and_then(|state| {
                let mut enigo = state
                    .0
                    .lock()
                    .map_err(|e| format!("Failed to lock Enigo: {e}"))?;
                input::send_copy_ctrl_c(&mut enigo)
            });
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to run the copy shortcut: {e}"))?;
    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "The copy shortcut timed out".to_string())??;

    let started = Instant::now();
    loop {
        let changed = match (before, clipboard_change_count()) {
            (Some(before), Some(now)) => now != before,
            // No change counter on this platform: compare the text.
            _ => app.clipboard().read_text().ok() != previous_clipboard,
        };
        if changed {
            break;
        }
        if started.elapsed() > COPY_TIMEOUT {
            debug!("Clipboard didn't change after the copy shortcut: nothing selected");
            return Ok(CopiedSelection {
                text: None,
                previous_clipboard,
            });
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // Some apps bump the change counter just before the data lands.
    std::thread::sleep(Duration::from_millis(30));

    let text = app
        .clipboard()
        .read_text()
        .ok()
        .filter(|t| !t.trim().is_empty());
    Ok(CopiedSelection {
        text,
        previous_clipboard,
    })
}

/// Put back the clipboard text from before [`copy_selected_text`].
pub fn restore_clipboard(app: &AppHandle, previous: Option<String>) {
    if let Some(text) = previous {
        if let Err(e) = app.clipboard().write_text(text) {
            warn!("Couldn't restore the clipboard: {e}");
        }
    }
}

/// Wait until no Shift, Control, Option/Alt or Command/Windows key is held.
/// Returns false on timeout.
fn wait_for_modifiers_released(timeout: Duration) -> bool {
    let started = Instant::now();
    while modifiers_held() {
        if started.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    true
}

#[cfg(target_os = "macos")]
fn modifiers_held() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceFlagsState(state_id: i32) -> u64;
    }
    // kCGEventSourceStateHIDSystemState: the physical keyboard.
    const HID_SYSTEM_STATE: i32 = 1;
    // kCGEventFlagMaskShift | Control | Alternate | Command.
    const MODIFIERS: u64 = 0x0002_0000 | 0x0004_0000 | 0x0008_0000 | 0x0010_0000;
    // SAFETY: a pure query of the current modifier flags.
    let flags = unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) };
    flags & MODIFIERS != 0
}

#[cfg(target_os = "windows")]
fn modifiers_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
        .iter()
        .any(|vk| {
            // SAFETY: a pure query of a virtual key's state. The high bit means
            // the key is down.
            (unsafe { GetAsyncKeyState(i32::from(vk.0)) } as u16) & 0x8000 != 0
        })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn modifiers_held() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn clipboard_change_count() -> Option<i64> {
    Some(objc2_app_kit::NSPasteboard::generalPasteboard().changeCount() as i64)
}

#[cfg(target_os = "windows")]
fn clipboard_change_count() -> Option<i64> {
    // SAFETY: a pure query with no arguments.
    Some(i64::from(unsafe {
        windows::Win32::System::DataExchange::GetClipboardSequenceNumber()
    }))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn clipboard_change_count() -> Option<i64> {
    None
}
