//! Persistent user PATH (HKCU `Environment` read-modify-write) + change broadcast.
//!
//! The pure rewrite (`compute_user_path`) is host-neutral and fully unit-tested
//! on Linux; the real registry write (`set_user_path`) is `#[cfg(windows)]` with
//! a non-windows stub. §2.2 mandates `REG_EXPAND_SZ` and forbids `setx` of a
//! constructed PATH (1024-char truncation).

use anyhow::Result;

/// Pure: build the new user PATH value. Prepend `new_bin`, drop any segment equal
/// to `prior_bin` or `new_bin` already present (idempotent, no duplicate), keep
/// every other segment verbatim and in order. NEVER truncates — the caller must
/// write the whole string via the registry, never `setx` (§2.2).
#[must_use]
pub fn compute_user_path(old: &str, new_bin: &str, prior_bin: Option<&str>) -> String {
    let mut segments: Vec<&str> = old
        .split(';')
        .filter(|s| !s.is_empty())
        .filter(|s| *s != new_bin && Some(*s) != prior_bin)
        .collect();
    let mut out = Vec::with_capacity(segments.len() + 1);
    out.push(new_bin);
    out.append(&mut segments);
    out.join(";")
}

#[cfg(windows)]
mod sys {
    use super::Result;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_READ, KEY_WRITE, REG_EXPAND_SZ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    fn to_utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read_user_path() -> Result<String> {
        unsafe {
            let mut hkey = HKEY::default();
            RegOpenKeyExW(HKEY_CURRENT_USER, w!("Environment"), 0, KEY_READ, &raw mut hkey).ok()?;
            let name = to_utf16("Path");
            // First call sizes the value (bytes).
            let mut size: u32 = 0;
            let _ =
                RegQueryValueExW(hkey, PCWSTR(name.as_ptr()), None, None, None, Some(&raw mut size));
            // A u16 buffer is 2-aligned; passing it down as *u8 is never over-aligned.
            let mut buf = vec![0u16; (size as usize / 2) + 1];
            let mut got = size;
            let r = RegQueryValueExW(
                hkey,
                PCWSTR(name.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr().cast::<u8>()),
                Some(&raw mut got),
            );
            let _ = RegCloseKey(hkey);
            if r.is_err() {
                return Ok(String::new()); // no user Path yet
            }
            let nchars = (got as usize / 2).min(buf.len());
            let end = buf[..nchars].iter().position(|&c| c == 0).unwrap_or(nchars);
            Ok(String::from_utf16_lossy(&buf[..end]))
        }
    }

    /// Read `HKCU\Environment\Path`, rewrite it (prepend `new_bin`, strip
    /// `prior_bin`), write it back as `REG_EXPAND_SZ`, then broadcast the change.
    ///
    /// # Errors
    /// Returns an error if the registry key cannot be opened or written.
    pub fn set_user_path(new_bin: &str, prior_bin: Option<&str>) -> Result<()> {
        let old = read_user_path()?;
        let next = super::compute_user_path(&old, new_bin, prior_bin);
        unsafe {
            let mut hkey = HKEY::default();
            RegOpenKeyExW(HKEY_CURRENT_USER, w!("Environment"), 0, KEY_WRITE, &raw mut hkey).ok()?;
            let data = to_utf16(&next);
            let bytes = std::slice::from_raw_parts(
                data.as_ptr().cast::<u8>(),
                data.len() * std::mem::size_of::<u16>(),
            );
            let r = RegSetValueExW(hkey, w!("Path"), 0, REG_EXPAND_SZ, Some(bytes));
            let _ = RegCloseKey(hkey);
            r.ok()?;
            // Broadcast so already-open shells/Explorer pick up the change.
            let env = to_utf16("Environment");
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(env.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Result;

    /// Non-windows stub so the crate compiles on the gnu/linux host.
    ///
    /// # Errors
    /// Always returns an error: persistent PATH writes are windows-only.
    pub fn set_user_path(_new_bin: &str, _prior_bin: Option<&str>) -> Result<()> {
        anyhow::bail!("set_user_path is only available on windows")
    }
}

pub use sys::set_user_path;
