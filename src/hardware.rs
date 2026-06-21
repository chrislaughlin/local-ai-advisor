use std::{env, fs, path::Path, process::Command, time::Duration};

use serde::{Deserialize, Serialize};
use sysinfo::{Disks, System};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_gb: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaStatus {
    pub installed: bool,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareProfile {
    pub os: String,
    pub cpu_model: String,
    pub architecture: String,
    pub cpu_cores: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_cpu_cores: Option<usize>,
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub disk_free_gb: f64,
    pub gpus: Vec<GpuInfo>,
    pub apple_silicon: bool,
    pub unified_memory: bool,
    pub ollama: OllamaStatus,
    pub llama_cpp_installed: bool,
    pub warnings: Vec<String>,
}

impl HardwareProfile {
    pub fn safe_usable_memory_gb(&self) -> f64 {
        let headroom = if self.total_ram_gb <= 8.5 {
            2.0
        } else if self.total_ram_gb <= 16.5 {
            4.0
        } else if self.total_ram_gb <= 24.5 {
            6.0
        } else {
            self.total_ram_gb * 0.25
        };
        (self.total_ram_gb - headroom).max(0.0)
    }

    pub fn max_vram_gb(&self) -> Option<f64> {
        self.gpus
            .iter()
            .filter_map(|gpu| gpu.vram_gb)
            .reduce(f64::max)
    }
}

pub async fn scan(client: &reqwest::Client) -> HardwareProfile {
    let mut system = System::new_all();
    system.refresh_all();

    let mut warnings = Vec::new();
    let os = match System::name().as_deref() {
        Some("Darwin") => "macOS".to_string(),
        Some(value) => value.to_string(),
        None => env::consts::OS.to_string(),
    };
    let architecture = env::consts::ARCH.to_string();
    let mut cpu_model = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    if cfg!(target_os = "macos") {
        if let Some(value) = command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"]) {
            if !value.trim().is_empty() {
                cpu_model = value.trim().to_string();
            }
        }
    }

    let mut total_ram_gb = bytes_to_gb(system.total_memory());
    let mut available_ram_gb = bytes_to_gb(system.available_memory());
    if cfg!(target_os = "linux") {
        if let Some((total, available)) = linux_memory_info() {
            total_ram_gb = total;
            available_ram_gb = available;
        } else {
            warnings.push("Could not parse /proc/meminfo; using sysinfo memory values.".into());
        }
    }

    let disks = Disks::new_with_refreshed_list();
    let disk_free_gb = disks
        .list()
        .iter()
        .map(|disk| disk.available_space())
        .max()
        .map(bytes_to_gb)
        .unwrap_or_else(|| {
            warnings.push("Disk free space was not available.".into());
            0.0
        });

    let apple_silicon = cfg!(target_os = "macos")
        && (architecture == "aarch64"
            || architecture == "arm64"
            || cpu_model.to_ascii_lowercase().contains("apple"));
    let unified_memory = apple_silicon;
    let gpus = detect_gpus(&mut warnings);

    let ollama_installed = executable_exists("ollama");
    let ollama_running = client
        .get("http://127.0.0.1:11434/api/tags")
        .timeout(Duration::from_millis(800))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false);

    HardwareProfile {
        os,
        cpu_model,
        architecture,
        cpu_cores: system.cpus().len(),
        physical_cpu_cores: system.physical_core_count(),
        total_ram_gb: round1(total_ram_gb),
        available_ram_gb: round1(available_ram_gb),
        disk_free_gb: round1(disk_free_gb),
        gpus,
        apple_silicon,
        unified_memory,
        ollama: OllamaStatus {
            installed: ollama_installed,
            running: ollama_running,
        },
        llama_cpp_installed: ["llama-cli", "llama-server", "main"]
            .iter()
            .any(|name| executable_exists(name)),
        warnings,
    }
}

fn detect_gpus(warnings: &mut Vec<String>) -> Vec<GpuInfo> {
    if let Some(output) = command_stdout(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    ) {
        let values: Vec<GpuInfo> = output
            .lines()
            .filter_map(|line| {
                let (name, memory) = line.rsplit_once(',')?;
                let memory_mb: f64 = memory.trim().parse().ok()?;
                Some(GpuInfo {
                    name: name.trim().to_string(),
                    vram_gb: Some(round1(memory_mb / 1024.0)),
                })
            })
            .collect();
        if !values.is_empty() {
            return values;
        }
    }

    if cfg!(target_os = "macos") {
        if let Some(output) = command_stdout(
            "system_profiler",
            &["SPDisplaysDataType", "-detailLevel", "mini"],
        ) {
            let values: Vec<GpuInfo> = output
                .lines()
                .filter_map(|line| line.trim().strip_prefix("Chipset Model:"))
                .map(|name| GpuInfo {
                    name: name.trim().to_string(),
                    vram_gb: None,
                })
                .collect();
            if !values.is_empty() {
                return values;
            }
        }
    }

    if cfg!(target_os = "linux") {
        if let Some(output) = command_stdout("lspci", &[]) {
            let values: Vec<GpuInfo> = output
                .lines()
                .filter(|line| {
                    let lower = line.to_ascii_lowercase();
                    lower.contains("vga compatible controller") || lower.contains("3d controller")
                })
                .map(|line| GpuInfo {
                    name: line
                        .split_once(": ")
                        .map(|(_, value)| value)
                        .unwrap_or(line)
                        .to_string(),
                    vram_gb: None,
                })
                .collect();
            if !values.is_empty() {
                return values;
            }
        }
    }

    if cfg!(any(target_os = "linux", target_os = "macos")) {
        warnings
            .push("GPU/VRAM detection is best-effort; no discrete GPU details were found.".into());
    }
    Vec::new()
}

fn linux_memory_info() -> Option<(f64, f64)> {
    let contents = fs::read_to_string("/proc/meminfo").ok()?;
    let value = |key: &str| -> Option<u64> {
        contents.lines().find_map(|line| {
            let remainder = line.strip_prefix(key)?;
            remainder.split_whitespace().next()?.parse::<u64>().ok()
        })
    };
    let total_bytes = value("MemTotal:")? * 1024;
    let available_bytes = value("MemAvailable:")? * 1024;
    Some((bytes_to_gb(total_bytes), bytes_to_gb(available_bytes)))
}

pub fn executable_exists(name: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd"]
    } else {
        &[""]
    };
    env::split_paths(&paths).any(|directory| {
        extensions
            .iter()
            .map(|extension| directory.join(format!("{name}{extension}")))
            .any(|path| is_executable_file(&path))
    })
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / 1024_f64.powi(3)
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
pub fn mock_profile(
    total_ram_gb: f64,
    apple_silicon: bool,
    vram_gb: Option<f64>,
) -> HardwareProfile {
    HardwareProfile {
        os: if apple_silicon { "macOS" } else { "Linux" }.into(),
        cpu_model: if apple_silicon {
            "Apple M4 Pro"
        } else {
            "Test CPU"
        }
        .into(),
        architecture: if apple_silicon { "aarch64" } else { "x86_64" }.into(),
        cpu_cores: 8,
        physical_cpu_cores: Some(8),
        total_ram_gb,
        available_ram_gb: total_ram_gb * 0.75,
        disk_free_gb: 100.0,
        gpus: vram_gb
            .map(|vram| {
                vec![GpuInfo {
                    name: "NVIDIA Test GPU".into(),
                    vram_gb: Some(vram),
                }]
            })
            .unwrap_or_default(),
        apple_silicon,
        unified_memory: apple_silicon,
        ollama: OllamaStatus {
            installed: true,
            running: true,
        },
        llama_cpp_installed: false,
        warnings: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserves_required_os_headroom() {
        assert_eq!(mock_profile(8.0, false, None).safe_usable_memory_gb(), 6.0);
        assert_eq!(
            mock_profile(16.0, false, None).safe_usable_memory_gb(),
            12.0
        );
        assert_eq!(mock_profile(24.0, true, None).safe_usable_memory_gb(), 18.0);
        assert_eq!(
            mock_profile(64.0, false, Some(24.0)).safe_usable_memory_gb(),
            48.0
        );
    }
}
