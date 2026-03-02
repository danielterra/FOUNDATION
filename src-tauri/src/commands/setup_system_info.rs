#[derive(Debug)]
pub(super) struct InternalProcessorInfo {
    pub(super) model: String,
    pub(super) cores: Option<i64>,
    pub(super) architecture: String,
}

#[derive(Debug)]
pub(super) struct InternalMemoryInfo {
    pub(super) capacity_gb: i64,
    pub(super) memory_type: String,
}

#[derive(Debug)]
pub(super) struct InternalOperatingSystemInfo {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) kernel: String,
}

#[derive(Debug)]
pub(super) struct InternalLocaleInfo {
    pub(super) language: String,
    pub(super) locale: String,
    pub(super) country: String,
}

/// Get detailed CPU information
pub(super) fn get_cpu_info() -> InternalProcessorInfo {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let _model = if let Ok(output) = Command::new("sysctl")
            .args(&["-n", "machdep.cpu.brand_string"])
            .output()
        {
            String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "Unknown CPU".to_string())
                .trim()
                .to_string()
        } else {
            "Unknown CPU".to_string()
        };

        let cores = if let Ok(output) = Command::new("sysctl")
            .args(&["-n", "hw.physicalcpu"])
            .output()
        {
            String::from_utf8(output.stdout)
                .ok()
                .and_then(|s| s.trim().parse::<i64>().ok())
        } else {
            None
        };

        let architecture = std::env::consts::ARCH.to_string();

        return InternalProcessorInfo {
            model: _model,
            cores,
            architecture,
        };
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;

        let mut _model = "Unknown CPU".to_string();
        let mut cores = None;

        if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
            for line in content.lines() {
                if line.starts_with("model name") {
                    if let Some(cpu) = line.split(':').nth(1) {
                        _model = cpu.trim().to_string();
                    }
                }
                if line.starts_with("cpu cores") {
                    if let Some(core_str) = line.split(':').nth(1) {
                        cores = core_str.trim().parse::<i64>().ok();
                    }
                }
            }
        }

        let architecture = std::env::consts::ARCH.to_string();

        return InternalProcessorInfo {
            model: _model,
            cores,
            architecture,
        };
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let _model = if let Ok(output) = Command::new("wmic")
            .args(&["cpu", "get", "name"])
            .output()
        {
            if let Ok(cpu) = String::from_utf8(output.stdout) {
                let lines: Vec<&str> = cpu.lines().collect();
                if lines.len() > 1 {
                    lines[1].trim().to_string()
                } else {
                    "Unknown CPU".to_string()
                }
            } else {
                "Unknown CPU".to_string()
            }
        } else {
            "Unknown CPU".to_string()
        };

        let cores = if let Ok(output) = Command::new("wmic")
            .args(&["cpu", "get", "NumberOfCores"])
            .output()
        {
            if let Ok(core_str) = String::from_utf8(output.stdout) {
                let lines: Vec<&str> = core_str.lines().collect();
                if lines.len() > 1 {
                    lines[1].trim().parse::<i64>().ok()
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let architecture = std::env::consts::ARCH.to_string();

        return InternalProcessorInfo {
            model: _model,
            cores,
            architecture,
        };
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        InternalProcessorInfo {
            model: "Unknown CPU".to_string(),
            cores: None,
            architecture: std::env::consts::ARCH.to_string(),
        }
    }
}

/// Get detailed RAM information
pub(super) fn get_memory_info() -> InternalMemoryInfo {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let capacity_gb = if let Ok(output) = Command::new("sysctl")
            .args(&["-n", "hw.memsize"])
            .output()
        {
            if let Ok(ram_str) = String::from_utf8(output.stdout) {
                if let Ok(ram_bytes) = ram_str.trim().parse::<u64>() {
                    (ram_bytes / 1_073_741_824) as i64
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };

        let memory_type = "Unknown".to_string();

        return InternalMemoryInfo {
            capacity_gb,
            memory_type,
        };
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;

        let capacity_gb = if let Ok(content) = fs::read_to_string("/proc/meminfo") {
            let mut gb = 0;
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(ram_kb) = line.split_whitespace().nth(1) {
                        if let Ok(ram_kb) = ram_kb.parse::<u64>() {
                            gb = (ram_kb / 1_048_576) as i64;
                            break;
                        }
                    }
                }
            }
            gb
        } else {
            0
        };

        let memory_type = "Unknown".to_string();

        return InternalMemoryInfo {
            capacity_gb,
            memory_type,
        };
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let capacity_gb = if let Ok(output) = Command::new("wmic")
            .args(&["computersystem", "get", "totalphysicalmemory"])
            .output()
        {
            if let Ok(ram_str) = String::from_utf8(output.stdout) {
                let lines: Vec<&str> = ram_str.lines().collect();
                if lines.len() > 1 {
                    if let Ok(ram_bytes) = lines[1].trim().parse::<u64>() {
                        (ram_bytes / 1_073_741_824) as i64
                    } else {
                        0
                    }
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };

        let memory_type = if let Ok(output) = Command::new("wmic")
            .args(&["memorychip", "get", "MemoryType"])
            .output()
        {
            if let Ok(type_str) = String::from_utf8(output.stdout) {
                let lines: Vec<&str> = type_str.lines().collect();
                if lines.len() > 1 {
                    // Memory type codes: 20=DDR, 21=DDR2, 24=DDR3, 26=DDR4, 34=DDR5
                    match lines[1].trim() {
                        "20" => "DDR".to_string(),
                        "21" => "DDR2".to_string(),
                        "24" => "DDR3".to_string(),
                        "26" => "DDR4".to_string(),
                        "34" => "DDR5".to_string(),
                        _ => "Unknown".to_string(),
                    }
                } else {
                    "Unknown".to_string()
                }
            } else {
                "Unknown".to_string()
            }
        } else {
            "Unknown".to_string()
        };

        return InternalMemoryInfo {
            capacity_gb,
            memory_type,
        };
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        InternalMemoryInfo {
            capacity_gb: 0,
            memory_type: "Unknown".to_string(),
        }
    }
}

/// Get detailed operating system information
pub(super) fn get_os_info() -> InternalOperatingSystemInfo {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let name = "macOS".to_string();

        let version = if let Ok(output) = Command::new("sw_vers")
            .args(&["-productVersion"])
            .output()
        {
            String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "Unknown".to_string())
                .trim()
                .to_string()
        } else {
            "Unknown".to_string()
        };

        let kernel = if let Ok(output) = Command::new("uname")
            .args(&["-r"])
            .output()
        {
            let kernel_version = String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "Unknown".to_string())
                .trim()
                .to_string();
            format!("Darwin {}", kernel_version)
        } else {
            "Darwin".to_string()
        };

        return InternalOperatingSystemInfo {
            name,
            version,
            kernel,
        };
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;

        let name = if let Ok(content) = fs::read_to_string("/etc/os-release") {
            let mut distro_name = "Linux".to_string();
            for line in content.lines() {
                if line.starts_with("NAME=") {
                    if let Some(name_val) = line.strip_prefix("NAME=") {
                        distro_name = name_val.trim_matches('"').to_string();
                        break;
                    }
                }
            }
            distro_name
        } else {
            "Linux".to_string()
        };

        let version = if let Ok(content) = fs::read_to_string("/etc/os-release") {
            let mut version_str = "Unknown".to_string();
            for line in content.lines() {
                if line.starts_with("VERSION_ID=") {
                    if let Some(ver) = line.strip_prefix("VERSION_ID=") {
                        version_str = ver.trim_matches('"').to_string();
                        break;
                    }
                }
            }
            version_str
        } else {
            "Unknown".to_string()
        };

        let kernel = if let Ok(output) = std::process::Command::new("uname")
            .args(&["-r"])
            .output()
        {
            let kernel_version = String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "Unknown".to_string())
                .trim()
                .to_string();
            format!("Linux {}", kernel_version)
        } else {
            "Linux".to_string()
        };

        return InternalOperatingSystemInfo {
            name,
            version,
            kernel,
        };
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let name = "Windows".to_string();

        let version = if let Ok(output) = Command::new("cmd")
            .args(&["/C", "ver"])
            .output()
        {
            String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "Unknown".to_string())
                .trim()
                .to_string()
        } else {
            "Unknown".to_string()
        };

        let kernel = "NT".to_string();

        return InternalOperatingSystemInfo {
            name,
            version,
            kernel,
        };
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        InternalOperatingSystemInfo {
            name: std::env::consts::OS.to_string(),
            version: "Unknown".to_string(),
            kernel: "Unknown".to_string(),
        }
    }
}

/// Get locale information from the system
pub(super) fn get_locale_info() -> InternalLocaleInfo {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let locale = if let Ok(output) = Command::new("defaults")
            .args(&["read", "-g", "AppleLocale"])
            .output()
        {
            String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "en_US".to_string())
                .trim()
                .to_string()
        } else {
            "en_US".to_string()
        };

        let parts: Vec<&str> = locale.split('_').collect();
        let language = match parts.get(0).map(|s| *s) {
            Some("en") => "English",
            Some("pt") => "Portuguese",
            Some("es") => "Spanish",
            Some("fr") => "French",
            Some("de") => "German",
            Some("it") => "Italian",
            Some("ja") => "Japanese",
            Some("zh") => "Chinese",
            _ => "English",
        }.to_string();

        let country = match parts.get(1).map(|s| *s) {
            Some("US") => "United States",
            Some("BR") => "Brazil",
            Some("GB") => "United Kingdom",
            Some("CA") => "Canada",
            Some("AU") => "Australia",
            Some("ES") => "Spain",
            Some("MX") => "Mexico",
            Some("FR") => "France",
            Some("DE") => "Germany",
            Some("IT") => "Italy",
            Some("JP") => "Japan",
            Some("CN") => "China",
            _ => "United States",
        }.to_string();

        return InternalLocaleInfo {
            language,
            locale,
            country,
        };
    }

    #[cfg(target_os = "linux")]
    {
        use std::env;

        let locale = env::var("LANG")
            .unwrap_or_else(|_| "en_US.UTF-8".to_string())
            .split('.')
            .next()
            .unwrap_or("en_US")
            .to_string();

        let parts: Vec<&str> = locale.split('_').collect();
        let language = match parts.get(0).map(|s| *s) {
            Some("en") => "English",
            Some("pt") => "Portuguese",
            Some("es") => "Spanish",
            Some("fr") => "French",
            Some("de") => "German",
            Some("it") => "Italian",
            Some("ja") => "Japanese",
            Some("zh") => "Chinese",
            _ => "English",
        }.to_string();

        let country = match parts.get(1).map(|s| *s) {
            Some("US") => "United States",
            Some("BR") => "Brazil",
            Some("GB") => "United Kingdom",
            Some("CA") => "Canada",
            Some("AU") => "Australia",
            Some("ES") => "Spain",
            Some("MX") => "Mexico",
            Some("FR") => "France",
            Some("DE") => "Germany",
            Some("IT") => "Italy",
            Some("JP") => "Japan",
            Some("CN") => "China",
            _ => "United States",
        }.to_string();

        return InternalLocaleInfo {
            language,
            locale,
            country,
        };
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let locale = if let Ok(output) = Command::new("powershell")
            .args(&["-Command", "(Get-Culture).Name"])
            .output()
        {
            String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "en-US".to_string())
                .trim()
                .replace('-', "_")
        } else {
            "en_US".to_string()
        };

        let parts: Vec<&str> = locale.split('_').collect();
        let language = match parts.get(0).map(|s| *s) {
            Some("en") => "English",
            Some("pt") => "Portuguese",
            Some("es") => "Spanish",
            Some("fr") => "French",
            Some("de") => "German",
            Some("it") => "Italian",
            Some("ja") => "Japanese",
            Some("zh") => "Chinese",
            _ => "English",
        }.to_string();

        let country = match parts.get(1).map(|s| *s) {
            Some("US") => "United States",
            Some("BR") => "Brazil",
            Some("GB") => "United Kingdom",
            Some("CA") => "Canada",
            Some("AU") => "Australia",
            Some("ES") => "Spain",
            Some("MX") => "Mexico",
            Some("FR") => "France",
            Some("DE") => "Germany",
            Some("IT") => "Italy",
            Some("JP") => "Japan",
            Some("CN") => "China",
            _ => "United States",
        }.to_string();

        return InternalLocaleInfo {
            language,
            locale,
            country,
        };
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        InternalLocaleInfo {
            language: "English".to_string(),
            locale: "en_US".to_string(),
            country: "United States".to_string(),
        }
    }
}
