use crate::Error;

/// Enforces a peak RSS memory limit.
///
/// Call [`check`](MemoryGuard::check) at processing checkpoints.  Returns
/// [`Error::MemoryExceeded`] if the limit is breached so the caller can abort
/// cleanly rather than hitting OOM.
///
/// Platform support:
/// - **Linux** — reads `VmRSS` from `/proc/self/status`
/// - **macOS** — parses `vm_stat` (development only; Lambda runs Linux)
/// - **Other** — silently skips the check (fail-open)
pub struct MemoryGuard {
    limit_bytes: u64,
}

impl MemoryGuard {
    /// Create a guard with the given limit.  Pass `u64::MAX` to disable.
    #[must_use]
    pub fn new(limit_bytes: u64) -> Self {
        Self { limit_bytes }
    }

    /// Return the current RSS in bytes, or `None` on unsupported platforms.
    #[must_use]
    pub fn current_rss_bytes() -> Option<u64> {
        #[cfg(target_os = "linux")]
        return rss_linux();

        #[cfg(target_os = "macos")]
        return rss_macos();

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        None
    }

    /// Return `Ok(())` if RSS is within the limit or unreadable (fail-open).
    ///
    /// # Errors
    ///
    /// Returns [`Error::MemoryExceeded`] when measured RSS exceeds the limit.
    pub fn check(&self) -> Result<(), Error> {
        if let Some(rss) = Self::current_rss_bytes() {
            if rss > self.limit_bytes {
                return Err(Error::MemoryExceeded {
                    used_mb: rss / (1024 * 1024),
                    limit_mb: self.limit_bytes / (1024 * 1024),
                });
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn rss_linux() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            // "VmRSS:\t  12345 kB"
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

/// Approximate RSS on macOS by summing active + wired pages from `vm_stat`.
///
/// We use a subprocess rather than `libc::getrusage` to keep this crate
/// free of `unsafe` blocks.  This path is only taken on development machines
/// (Lambda always runs Linux).
#[cfg(target_os = "macos")]
fn rss_macos() -> Option<u64> {
    let output = std::process::Command::new("vm_stat").output().ok()?;
    let text = std::str::from_utf8(&output.stdout).ok()?;

    let page_size: u64 = text
        .lines()
        .next()
        .and_then(|hdr| {
            let start = hdr.find("page size of ")? + "page size of ".len();
            let rest = &hdr[start..];
            rest[..rest.find(" bytes")?].parse().ok()
        })
        .unwrap_or(4096);

    let mut active: u64 = 0;
    let mut wired: u64 = 0;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Pages active:") {
            active = v.trim().trim_end_matches('.').parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("Pages wired down:") {
            wired = v.trim().trim_end_matches('.').parse().unwrap_or(0);
        }
    }
    Some((active + wired) * page_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_passes_under_limit() {
        assert!(MemoryGuard::new(u64::MAX).check().is_ok());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn guard_fails_over_zero_limit() {
        assert!(MemoryGuard::new(0).check().is_err());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn rss_is_positive_on_supported_platforms() {
        let rss = MemoryGuard::current_rss_bytes();
        assert!(rss.is_some());
        assert!(rss.unwrap() > 0);
    }
}
