use crate::Error;

/// Enforces a configurable peak RSS memory limit.
///
/// Call [`MemoryGuard::check`] at checkpoints during processing.  If the
/// current RSS exceeds the limit, [`Error::MemoryExceeded`] is returned and
/// the caller should abort the operation immediately.
///
/// # Platform support
///
/// | Platform | Source                     | Notes                          |
/// |----------|----------------------------|--------------------------------|
/// | Linux    | `/proc/self/status` `VmRSS`  | Primary target (Lambda)        |
/// | macOS    | `vm_stat` output           | Development machines           |
/// | Windows  | Not available              | Check is skipped (fail-open)   |
/// | Other    | Not available              | Check is skipped (fail-open)   |
///
/// The fail-open behaviour on unsupported platforms means memory-limit
/// enforcement is **not** a security boundary on those platforms.
pub struct MemoryGuard {
    limit_bytes: u64,
}

impl MemoryGuard {
    /// Create a new guard with the given RSS limit in bytes.
    ///
    /// Pass `u64::MAX` to disable the guard without removing call sites.
    #[must_use]
    pub fn new(limit_bytes: u64) -> Self {
        Self { limit_bytes }
    }

    /// Return the current resident set size (RSS) in bytes, if determinable.
    ///
    /// Returns `None` when the value cannot be obtained on this platform.
    #[must_use]
    pub fn current_rss_bytes() -> Option<u64> {
        #[cfg(target_os = "linux")]
        return rss_linux();

        #[cfg(target_os = "macos")]
        return rss_macos();

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        None
    }

    /// Check whether the current RSS is within the configured limit.
    ///
    /// Returns `Ok(())` when:
    /// - RSS is below the limit, **or**
    /// - RSS cannot be determined on this platform (fail-open).
    ///
    /// # Errors
    ///
    /// Returns [`Error::MemoryExceeded`] if the measured RSS exceeds the
    /// configured limit.
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

// ── Platform implementations ──────────────────────────────────────────────

/// Read `VmRSS` from `/proc/self/status` on Linux.
///
/// This is available on all Linux kernels and does not require any syscall
/// beyond `read(2)`.
#[cfg(target_os = "linux")]
fn rss_linux() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            // Line format: "VmRSS:\t  12345 kB"
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

/// Approximate active memory on macOS by parsing `vm_stat` output.
///
/// We deliberately avoid `libc::getrusage` here so that this crate contains
/// zero `unsafe` blocks.  On macOS `vm_stat` is available on all supported
/// versions (10.9+).  For Lambda workloads (Linux) this path is never taken.
#[cfg(target_os = "macos")]
fn rss_macos() -> Option<u64> {
    let output = std::process::Command::new("vm_stat").output().ok()?;
    let text = std::str::from_utf8(&output.stdout).ok()?;

    // First, find the page size from the header line, e.g.
    // "Mach Virtual Memory Statistics: (page size of 16384 bytes)"
    let page_size: u64 = text
        .lines()
        .next()
        .and_then(|header| {
            let start = header.find("page size of ")? + "page size of ".len();
            let rest = &header[start..];
            let end = rest.find(" bytes")?;
            rest[..end].parse().ok()
        })
        .unwrap_or(4096); // safe default

    // Sum "Pages active" and "Pages wired down" as a proxy for RSS.
    let mut active: u64 = 0;
    let mut wired: u64 = 0;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Pages active:") {
            active = rest.trim().trim_end_matches('.').parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("Pages wired down:") {
            wired = rest.trim().trim_end_matches('.').parse().unwrap_or(0);
        }
    }
    Some((active + wired) * page_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_passes_under_limit() {
        let guard = MemoryGuard::new(u64::MAX);
        assert!(guard.check().is_ok());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn guard_fails_over_zero_limit() {
        // A 0-byte limit always fails when we can measure RSS.
        let guard = MemoryGuard::new(0);
        assert!(guard.check().is_err());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn rss_is_positive_on_supported_platforms() {
        let rss = MemoryGuard::current_rss_bytes();
        assert!(rss.is_some(), "expected RSS reading on this platform");
        assert!(rss.unwrap() > 0, "RSS should be > 0 for a running process");
    }
}
