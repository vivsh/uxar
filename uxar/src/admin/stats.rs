use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, ProcessesToUpdate, System};

/// Cached stats with monotonic timestamp
struct CachedStats {
    stats: LiveStats,
    cached_at_mono: Instant,
}

/// Global cache for stats (throttled refresh)
static STATS_CACHE: Lazy<RwLock<Option<CachedStats>>> = Lazy::new(|| RwLock::new(None));

/// Minimum seconds between refreshes (prevents hammering)
const MIN_REFRESH_INTERVAL_SECS: u64 = 5;

/// Real-time snapshot of server resource usage and system metrics
/// WARNING: capture() blocks for ~100ms; use get() with cache for repeated calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveStats {
    /// Timestamp of when these stats were captured (seconds since UNIX epoch)
    pub timestamp: u64,
    
    // System-wide metrics
    /// Overall CPU usage percentage (per sysinfo semantics)
    pub cpu_usage: f32,
    /// Total system memory in bytes
    pub total_memory_bytes: u64,
    /// Used system memory in bytes
    pub used_memory_bytes: u64,
    /// Available system memory in bytes
    pub available_memory_bytes: u64,
    /// Memory usage percentage (0.0 to 100.0)
    pub memory_usage_percent: f32,
    /// Total swap memory in bytes
    pub total_swap_bytes: u64,
    /// Used swap memory in bytes
    pub used_swap_bytes: u64,
    /// System uptime in seconds
    pub uptime: u64,
    /// Number of logical CPU cores
    pub cpu_count: usize,
    /// System load average (1 minute) - Unix only
    pub load_average_1: Option<f64>,
    /// System load average (5 minutes) - Unix only
    pub load_average_5: Option<f64>,
    /// System load average (15 minutes) - Unix only
    pub load_average_15: Option<f64>,
    
    // Process-specific metrics (current binary)
    /// Current process CPU usage percentage (may exceed 100 on multicore per sysinfo)
    pub process_cpu_usage: f32,
    /// Current process memory usage (RSS) in bytes
    pub process_memory_bytes: u64,
    /// Current process memory usage percentage of total system memory
    pub process_memory_percent: f32,
    /// Current process virtual memory in bytes
    pub process_virtual_memory_bytes: u64,
    /// Process start time in seconds since system boot (sysinfo convention, not UNIX epoch)
    pub process_start_time_since_boot_secs: u64,
    /// Process uptime in seconds (computed from system uptime - start_time)
    pub process_uptime: u64,
    /// Process ID
    pub process_pid: u32,
}

impl LiveStats {
    /// Get stats with in-memory cache (throttled to MIN_REFRESH_INTERVAL_SECS)
    /// Returns cached data if recent, otherwise refreshes and caches.
    /// Uses monotonic time to avoid system clock skew issues.
    pub fn get() -> Result<Self, Box<dyn std::error::Error>> {
        let now_mono = Instant::now();
        
        // Check cache first (monotonic time for robustness)
        {
            let cache = STATS_CACHE.read();
            if let Some(cached) = cache.as_ref() {
                if now_mono.duration_since(cached.cached_at_mono).as_secs() < MIN_REFRESH_INTERVAL_SECS {
                    return Ok(cached.stats.clone());
                }
            }
        }
        
        // Cache miss or stale - refresh
        let stats = Self::capture()?;
        
        // Update cache with monotonic timestamp
        {
            let mut cache = STATS_CACHE.write();
            *cache = Some(CachedStats {
                stats: stats.clone(),
                cached_at_mono: now_mono,
            });
        }
        
        Ok(stats)
    }
    
    /// Force refresh stats (bypasses cache)
    /// Note: This takes ~100-150ms due to CPU sampling. Only call when needed.
    pub fn capture() -> Result<Self, Box<dyn std::error::Error>> {
        let mut sys = System::new();
        
        // Refresh only what we need
        sys.refresh_memory();
        sys.refresh_cpu_all();
        
        // Brief wait for CPU usage accuracy
        std::thread::sleep(Duration::from_millis(100));
        sys.refresh_cpu_all();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();

        // System-wide metrics (sysinfo returns KiB, convert to bytes)
        let total_memory_kib = sys.total_memory();
        let used_memory_kib = sys.used_memory();
        let available_memory_kib = sys.available_memory();
        
        let total_memory_bytes = total_memory_kib * 1024;
        let used_memory_bytes = used_memory_kib * 1024;
        let available_memory_bytes = available_memory_kib * 1024;
        
        let memory_usage_percent = if total_memory_kib > 0 {
            (used_memory_kib as f32 / total_memory_kib as f32) * 100.0
        } else {
            0.0
        };

        let cpu_usage = sys.global_cpu_usage();
        let cpu_count = sys.cpus().len();

        // Load average only on Unix
        let (load_average_1, load_average_5, load_average_15) = {
            #[cfg(unix)]
            {
                let load_avg = System::load_average();
                (Some(load_avg.one), Some(load_avg.five), Some(load_avg.fifteen))
            }
            #[cfg(not(unix))]
            {
                (None, None, None)
            }
        };

        // Process-specific metrics
        let current_pid = Pid::from_u32(std::process::id());
        sys.refresh_processes(ProcessesToUpdate::Some(&[current_pid]), false);

        let system_uptime = System::uptime();

        let (process_cpu_usage, process_memory_kib, process_virtual_memory_kib, 
             process_start_time) = sys
            .process(current_pid)
            .map(|p| {
                (
                    p.cpu_usage(),
                    p.memory(),
                    p.virtual_memory(),
                    p.start_time(),
                )
            })
            .unwrap_or((0.0, 0, 0, 0));

        let process_memory_bytes = process_memory_kib * 1024;
        let process_virtual_memory_bytes = process_virtual_memory_kib * 1024;

        let process_memory_percent = if total_memory_kib > 0 {
            (process_memory_kib as f32 / total_memory_kib as f32) * 100.0
        } else {
            0.0
        };

        // Process uptime: sysinfo start_time is seconds since boot
        let process_uptime = system_uptime.saturating_sub(process_start_time);

        Ok(Self {
            timestamp,
            cpu_usage,
            total_memory_bytes,
            used_memory_bytes,
            available_memory_bytes,
            memory_usage_percent,
            total_swap_bytes: sys.total_swap() * 1024,
            used_swap_bytes: sys.used_swap() * 1024,
            uptime: system_uptime,
            cpu_count,
            load_average_1,
            load_average_5,
            load_average_15,
            process_cpu_usage,
            process_memory_bytes,
            process_memory_percent,
            process_virtual_memory_bytes,
            process_start_time_since_boot_secs: process_start_time,
            process_uptime,
            process_pid: std::process::id(),
        })
    }

    /// Format memory value to human-readable string (e.g., "1.5 GB")
    pub fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        const TB: u64 = GB * 1024;

        if bytes >= TB {
            format!("{:.2} TB", bytes as f64 / TB as f64)
        } else if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    /// Format uptime to human-readable string (e.g., "2d 5h 30m" or "45s")
    pub fn format_uptime(seconds: u64) -> String {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;

        if days > 0 {
            format!("{}d {}h {}m", days, hours, minutes)
        } else if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, secs)
        } else {
            format!("{}s", secs)
        }
    }
}