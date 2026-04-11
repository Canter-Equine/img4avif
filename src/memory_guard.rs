use crate::Error;

/// Enforces a peak RSS memory limit.
///
/// Call [`check`](MemoryGuard::check) at processing checkpoints.  Returns
/// [`Error::MemoryExceeded`] if the limit is breached so the caller can abort
/// cleanly rather than hitting OOM.
///
/// The guard measures the **increase** in RSS from the baseline recorded at
/// construction time, rather than the absolute RSS.  This prevents the
/// pre-existing process baseline (e.g. loaded shared libraries or other
/// in-flight tasks) from falsely triggering the limit.
///
/// Platform support:
/// - **Linux** — reads `VmRSS` from `/proc/self/status`
/// - **macOS** — parses `vm_stat` (development only; Lambda runs Linux)
/// - **Other** — silently skips the check (fail-open)
pub struct MemoryGuard {
    limit_bytes: u64,
    /// RSS snapshot taken when the guard was created, used as the delta baseline.
    baseline_bytes: u64,
}

impl MemoryGuard {
    /// Create a guard with the given limit.  Pass `u64::MAX` to disable.
    ///
    /// The current RSS is recorded as the baseline; [`check`](Self::check)
    /// will fire only when the RSS *increase* from this baseline meets or
    /// exceeds `limit_bytes`.
    #[must_use]
    pub fn new(limit_bytes: u64) -> Self {
        let baseline_bytes = Self::current_rss_bytes().unwrap_or(0);
        Self {
            limit_bytes,
            baseline_bytes,
        }
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

    /// Return `Ok(())` if the RSS increase since construction is within the
    /// limit, or if RSS is unreadable (fail-open).
    ///
    /// The check compares the *increase* in RSS from the baseline recorded at
    /// construction time against `limit_bytes`.  This prevents a high
    /// pre-existing process RSS (e.g. from other loaded libraries or parallel
    /// test threads) from falsely triggering the guard on small inputs.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MemoryExceeded`] when `current_rss − baseline_rss ≥ limit_bytes`.
    /// A limit of `0` therefore always triggers the guard (any delta meets the limit).
    pub fn check(&self) -> Result<(), Error> {
        if let Some(rss) = Self::current_rss_bytes() {
            let delta = rss.saturating_sub(self.baseline_bytes);
            if delta >= self.limit_bytes {
                return Err(Error::MemoryExceeded {
                    used_mb: delta / (1024 * 1024),
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
