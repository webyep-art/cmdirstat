// Copyright (c) 2024 webyep
// Licensed under the MIT License.

use std::path::PathBuf;

#[cfg(windows)]
pub fn get_available_drives() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
    let mut drives = Vec::new();
    let drive_mask = unsafe { GetLogicalDrives() };
    for i in 0..26 {
        if (drive_mask >> i) & 1 == 1 {
            let drive_letter = (b'A' + i as u8) as char;
            let path = format!("{}:\\", drive_letter);
            drives.push(PathBuf::from(path));
        }
    }
    drives
}

#[cfg(windows)]
pub fn detect_is_ssd(path: &std::path::Path) -> Option<bool> {
    use std::mem::{size_of, MaybeUninit};
    use std::ptr;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    // Find the drive letter (e.g. "C")
    let path_str = path.to_string_lossy();
    let drive_letter = if path_str.len() >= 2 && path_str.as_bytes()[1] == b':' {
        Some(path_str.chars().next().unwrap())
    } else {
        None
    };

    let drive_letter = drive_letter?;
    let device_path = format!("\\\\.\\{}:", drive_letter);
    let path_w = device_path.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();

    unsafe {
        let handle = CreateFileW(
            path_w.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            0,
        );

        if handle == INVALID_HANDLE_VALUE {
            return None;
        }

        // IOCTL_STORAGE_QUERY_PROPERTY = 0x002d1444
        const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002d1444;

        #[repr(C)]
        #[allow(non_snake_case)]
        struct STORAGE_PROPERTY_QUERY {
            PropertyId: u32,
            QueryType: u32,
            AdditionalParameters: [u8; 1],
        }

        #[repr(C)]
        #[allow(non_snake_case)]
        struct DEVICE_SEEK_PENALTY_DESCRIPTOR {
            Version: u32,
            Size: u32,
            IncursSeekPenalty: u8,
        }

        // StorageDeviceSeekPenaltyProperty = 7
        // PropertyStandardQuery = 0
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: 7,
            QueryType: 0,
            AdditionalParameters: [0; 1],
        };

        let mut descriptor = MaybeUninit::<DEVICE_SEEK_PENALTY_DESCRIPTOR>::uninit();
        let mut bytes_returned = 0;

        let success = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *const _,
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            descriptor.as_mut_ptr() as *mut _,
            size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            &mut bytes_returned,
            ptr::null_mut(),
        );

        windows_sys::Win32::Foundation::CloseHandle(handle);

        if success != 0 {
            let desc = descriptor.assume_init();
            // If it does NOT incur seek penalty, it's SSD
            Some(desc.IncursSeekPenalty == 0)
        } else {
            None
        }
    }
}

#[cfg(target_os = "linux")]
pub fn get_available_drives() -> Vec<PathBuf> {
    use std::fs::read_to_string;

    let mut drives = vec![PathBuf::from("/")];

    if let Ok(content) = read_to_string("/proc/mounts") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let (dev, mount_point, fstype) = (parts[0], parts[1], parts[2]);
                if (dev.starts_with("/dev/") || fstype == "zfs")
                    && !mount_point.starts_with("/proc")
                    && !mount_point.starts_with("/sys")
                    && !mount_point.starts_with("/dev")
                {
                    drives.push(PathBuf::from(mount_point));
                }
            }
        }
    }

    drives.sort();
    drives.dedup();
    drives
}

#[cfg(target_os = "linux")]
pub fn detect_is_ssd(path: &std::path::Path) -> Option<bool> {
    use std::fs::{canonicalize, read_to_string};

    let content = read_to_string("/proc/mounts").ok()?;
    let dev_path = content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            (parts.len() >= 2).then(|| (PathBuf::from(parts[1]), parts[0]))
        })
        .filter(|(mount_path, _)| path.starts_with(mount_path))
        .max_by_key(|(mount_path, _)| mount_path.as_os_str().len())
        .map(|(_, dev)| dev)?;

    let dev_canonical = canonicalize(dev_path).unwrap_or_else(|_| PathBuf::from(dev_path));
    let dev_name = dev_canonical.strip_prefix("/dev/").ok()?.to_str()?;

    let sys_path = PathBuf::from("/sys/class/block").join(dev_name);
    for q_path in [sys_path.join("queue/rotational"), sys_path.join("../queue/rotational")] {
        if let Ok(rotational) = read_to_string(q_path) {
            return Some(rotational.trim() == "0");
        }
    }

    None
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn get_available_drives() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn detect_is_ssd(_path: &std::path::Path) -> Option<bool> {
    None
}
