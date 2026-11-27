//! Resource usage for the dashboard: processor, memory, system drive and
//! uptime. Every read is a cheap kernel query; the dashboard samples only
//! while it is open.

use windows::{
    Win32::{
        Foundation::FILETIME,
        Storage::FileSystem::GetDiskFreeSpaceExW,
        System::{
            SystemInformation::{GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX},
            Threading::GetSystemTimes,
        },
    },
    core::HSTRING,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    /// 0–1.
    pub cpu: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
}

impl Usage {
    #[must_use]
    pub fn memory(&self) -> f32 {
        ratio(self.memory_used, self.memory_total)
    }

    #[must_use]
    pub fn disk(&self) -> f32 {
        ratio(self.disk_used, self.disk_total)
    }
}

fn ratio(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) as f32
    }
}

/// Processor time is a difference between two samples, so the sampler
/// keeps the previous one.
#[derive(Default)]
pub struct Sampler {
    previous: Option<(u64, u64)>,
}

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

impl Sampler {
    pub fn sample(&mut self) -> Usage {
        let mut usage = Usage {
            cpu: self.cpu(),
            ..Usage::default()
        };
        let mut memory = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: The struct is correctly sized.
        if unsafe { GlobalMemoryStatusEx(&mut memory) }.is_ok() {
            usage.memory_total = memory.ullTotalPhys;
            usage.memory_used = memory.ullTotalPhys.saturating_sub(memory.ullAvailPhys);
        }
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let (mut free, mut total) = (0u64, 0u64);
        // SAFETY: Out-pointers are valid u64s.
        if unsafe {
            GetDiskFreeSpaceExW(
                &HSTRING::from(format!("{drive}\\")),
                None,
                Some(&mut total),
                Some(&mut free),
            )
        }
        .is_ok()
        {
            usage.disk_total = total;
            usage.disk_used = total.saturating_sub(free);
        }
        usage
    }

    fn cpu(&mut self) -> f32 {
        let (mut idle, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: Out-pointers are valid FILETIMEs.
        if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_err() {
            return 0.0;
        }
        // Kernel time includes idle time.
        let busy_total = ticks(kernel) + ticks(user);
        let idle = ticks(idle);
        let result = self.previous.map_or(0.0, |(last_total, last_idle)| {
            let total = busy_total.saturating_sub(last_total);
            let idle = idle.saturating_sub(last_idle);
            if total == 0 {
                0.0
            } else {
                (1.0 - idle as f64 / total as f64).clamp(0.0, 1.0) as f32
            }
        });
        self.previous = Some((busy_total, idle));
        result
    }
}

/// Time since Windows started (not counting sleep is not possible here;
/// this includes it, as Task Manager's "Up time" does).
#[must_use]
pub fn uptime() -> std::time::Duration {
    // SAFETY: Read-only.
    std::time::Duration::from_millis(unsafe { GetTickCount64() })
}

/// `bytes` as "7.8 GiB".
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_in_range() {
        let mut sampler = Sampler::default();
        let _ = sampler.sample();
        let usage = sampler.sample();
        assert!((0.0..=1.0).contains(&usage.cpu));
        assert!(usage.memory_total > 0);
        assert!((0.0..=1.0).contains(&usage.memory()));
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(8 * 1024 * 1024 * 1024), "8.0 GiB");
    }
}
