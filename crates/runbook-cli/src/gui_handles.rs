//! The Windows std process API inherits all inheritable handles, even with null stdio.
//! Keep a captured CLI's original pipes out of the long-lived desktop process.
use std::{io, os::windows::io::AsRawHandle};
use windows_sys::Win32::Foundation::{
    GetHandleInformation, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
};

pub struct StandardHandles(Vec<(HANDLE, u32)>);

impl StandardHandles {
    pub fn protect() -> io::Result<Self> {
        let mut guard = Self(Vec::new());
        for handle in [
            io::stdin().as_raw_handle(),
            io::stdout().as_raw_handle(),
            io::stderr().as_raw_handle(),
        ] {
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                continue;
            }
            let mut flags = 0;
            // SAFETY: these are borrowed standard handles; this guard never closes them.
            if unsafe { GetHandleInformation(handle, &mut flags) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if flags & HANDLE_FLAG_INHERIT != 0 {
                // SAFETY: only change the inheritance bit during this single-threaded CLI launch.
                if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                guard.0.push((handle, flags));
            }
        }
        Ok(guard)
    }
}

impl Drop for StandardHandles {
    fn drop(&mut self) {
        for &(handle, flags) in &self.0 {
            // SAFETY: restore the borrowed handle's original inheritance bit after spawning.
            unsafe {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, flags);
            }
        }
    }
}
