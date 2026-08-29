#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchEnv {
    pub platform: &'static str,
    pub target_arch: &'static str,
    pub target_os: &'static str,
    pub target_family: &'static str,

    pub profile: &'static str,
    pub opt_level: &'static str,
    pub debug_assertions: bool,
    pub rustc_version: &'static str,
    pub host_triple: &'static str,
    pub git_commit: &'static str,
    pub git_dirty: bool,
    pub artisan_version: &'static str,

    pub rayon_threads: usize,

    pub available_parallelism: usize,
    pub cpu_brand: String,
    pub total_memory_mb: u64,
    pub os_description: String,

    pub timestamp_unix: u64,
}

const RUSTC_VERSION: &str = match option_env!("ARTISAN_RUSTC_VERSION") {
    Some(v) => v,
    None => "unknown",
};
const HOST_TRIPLE: &str = match option_env!("ARTISAN_HOST_TRIPLE") {
    Some(v) => v,
    None => "unknown",
};
const GIT_COMMIT: &str = match option_env!("ARTISAN_GIT_COMMIT") {
    Some(v) => v,
    None => "unknown",
};
const GIT_DIRTY: &str = match option_env!("ARTISAN_GIT_DIRTY") {
    Some(v) => v,
    None => "unknown",
};
const OPT_LEVEL: &str = match option_env!("ARTISAN_OPT_LEVEL") {
    Some(v) => v,
    None => "unknown",
};
const PROFILE: &str = match option_env!("ARTISAN_PROFILE") {
    Some(v) => v,
    None => "unknown",
};

impl BenchEnv {
    pub fn capture() -> Self {
        Self {
            platform: if cfg!(target_arch = "wasm32") { "browser-wasm" } else { "native" },
            target_arch: std::env::consts::ARCH,
            target_os: std::env::consts::OS,
            target_family: std::env::consts::FAMILY,
            profile: PROFILE,
            opt_level: OPT_LEVEL,
            debug_assertions: cfg!(debug_assertions),
            rustc_version: RUSTC_VERSION,
            host_triple: HOST_TRIPLE,
            git_commit: GIT_COMMIT,
            git_dirty: GIT_DIRTY == "true",
            artisan_version: env!("CARGO_PKG_VERSION"),
            rayon_threads: rayon::current_num_threads(),
            available_parallelism: available_parallelism(),
            cpu_brand: cpu_brand(),
            total_memory_mb: total_memory_mb(),
            os_description: os_description(),
            timestamp_unix: timestamp_unix(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn available_parallelism() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
fn available_parallelism() -> usize {

    0
}

#[cfg(not(target_arch = "wasm32"))]
fn timestamp_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
fn timestamp_unix() -> u64 {
    0
}

#[cfg(target_os = "windows")]
fn cpu_brand() -> String {
    std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".into())
}

#[cfg(target_os = "linux")]
fn cpu_brand() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|v| v.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn cpu_brand() -> String {
    "unknown".into()
}

#[cfg(target_os = "windows")]
fn total_memory_mb() -> u64 {

    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|b| b / 1024 / 1024)
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn total_memory_mb() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
        })
        .map(|kb| kb / 1024)
        .unwrap_or(0)
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn total_memory_mb() -> u64 {
    0
}

#[cfg(target_os = "windows")]
fn os_description() -> String {
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "$o=Get-CimInstance Win32_OperatingSystem; \"$($o.Caption) $($o.Version)\"",
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "windows".into())
}

#[cfg(target_os = "linux")]
fn os_description() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("PRETTY_NAME="))
                .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "linux".into())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn os_description() -> String {
    std::env::consts::OS.to_string()
}
