use serde::Serialize;
use tauri::State;
use rusqlite::Connection;

use crate::eavto::DbExecutor;
use crate::owl::{Individual, Object};

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub already_setup: bool,
    pub user: UserInfo,
    pub computer: ComputerInfo,
    pub foundation: FoundationInfo,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub iri: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessorInfo {
    pub iri: String,
    pub model: String,
    pub cores: Option<i64>,
    pub architecture: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    pub iri: String,
    pub capacity_gb: i64,
    pub memory_type: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatingSystemInfo {
    pub iri: String,
    pub name: String,
    pub version: String,
    pub kernel: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerInfo {
    pub iri: String,
    pub hostname: String,
    pub operating_system: OperatingSystemInfo,
    pub processor: ProcessorInfo,
    pub memory: MemoryInfo,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareReleaseInfo {
    pub iri: String,
    pub version_number: String,
    pub license_type: Option<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundationInfo {
    pub iri: String,
    pub release: SoftwareReleaseInfo,
}

/// Check if setup has been completed
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__check(
    executor: State<'_, DbExecutor>,
) -> Result<bool, String> {
    executor.read(|conn| {
        let foundation_instance = Individual::new("foundation:ThisFoundationInstance");
        foundation_instance.exists(conn)
            .map_err(|e| format!("Failed to check setup status: {}", e))
    }).await
}

/// Initialize setup: detect system, create instances, establish relationships
/// Should only be called when setup__check returns false
#[tauri::command]
#[allow(non_snake_case)]
pub async fn setup__init(
    user_name: String,
    email: Option<String>,
    executor: State<'_, DbExecutor>,
) -> Result<SetupResult, String> {
    // Setup involves writes, so we use the write executor
    let result_json = executor.write(move |conn| {

    // Don't check again - assume caller used setup__check first

    // Detect system information
    let hostname = hostname::get()
        .map_err(|e| format!("Failed to get hostname: {}", e))?
        .to_string_lossy()
        .to_string();

    let os_info = get_os_info();
    let cpu_info = get_cpu_info();
    let memory_info = get_memory_info();

    // Create Person instance with metadata
    let user = Individual::new("foundation:ThisUser");
    user.assert(conn, "foundation:Person", &user_name, "person", "setup")
        .map_err(|e| format!("Failed to create Person: {}", e))?;

    let name_obj = Object::Literal {
        value: user_name.clone(),
        datatype: Some("xsd:string".to_string()),
        language: Some("en".to_string()),
    };
    user.add_property(conn, "foundation:name", name_obj, "setup")
        .map_err(|e| format!("Failed to add user name: {}", e))?;

    if let Some(ref email_val) = email {
        let email_obj = Object::Literal {
            value: email_val.clone(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        };
        user.add_property(conn, "foundation:email", email_obj, "setup")
            .map_err(|e| format!("Failed to add user email: {}", e))?;
    }

    // Create Processor instance
    let processor = Individual::new("foundation:ThisProcessor");
    processor.assert(conn, "foundation:Processor", &cpu_info.model, "computer", "setup")
        .map_err(|e| format!("Failed to create Processor: {}", e))?;

    let model_obj = Object::Literal {
        value: cpu_info.model.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    processor.add_property(conn, "foundation:processorModel", model_obj, "setup")
        .map_err(|e| format!("Failed to add processor model: {}", e))?;

    if let Some(cores) = cpu_info.cores {
        processor.add_property(conn, "foundation:coreCount", Object::Integer(cores), "setup")
            .map_err(|e| format!("Failed to add core count: {}", e))?;
    }

    let arch_obj = Object::Literal {
        value: cpu_info.architecture.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    processor.add_property(conn, "foundation:architecture", arch_obj, "setup")
        .map_err(|e| format!("Failed to add architecture: {}", e))?;

    // Create Memory instance
    let memory = Individual::new("foundation:ThisMemory");
    let memory_label = format!("{}GB RAM", memory_info.capacity_gb);
    memory.assert(conn, "foundation:Memory", &memory_label, "computer", "setup")
        .map_err(|e| format!("Failed to create Memory: {}", e))?;

    memory.add_property(conn, "foundation:memoryCapacity", Object::Integer(memory_info.capacity_gb), "setup")
        .map_err(|e| format!("Failed to add memory capacity: {}", e))?;

    let mem_type_obj = Object::Literal {
        value: memory_info.memory_type.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    memory.add_property(conn, "foundation:memoryType", mem_type_obj, "setup")
        .map_err(|e| format!("Failed to add memory type: {}", e))?;

    // Create OperatingSystem instance
    let os = Individual::new("foundation:ThisOperatingSystem");
    let os_label = format!("{} {}", os_info.name, os_info.version);
    os.assert(conn, "foundation:OperatingSystem", &os_label, "computer", "setup")
        .map_err(|e| format!("Failed to create OperatingSystem: {}", e))?;

    let os_name_obj = Object::Literal {
        value: os_info.name.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osName", os_name_obj, "setup")
        .map_err(|e| format!("Failed to add OS name: {}", e))?;

    let os_version_obj = Object::Literal {
        value: os_info.version.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osVersion", os_version_obj, "setup")
        .map_err(|e| format!("Failed to add OS version: {}", e))?;

    let os_kernel_obj = Object::Literal {
        value: os_info.kernel.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    os.add_property(conn, "foundation:osKernel", os_kernel_obj, "setup")
        .map_err(|e| format!("Failed to add OS kernel: {}", e))?;

    // Create Computer instance with metadata
    let computer = Individual::new("foundation:ThisComputer");
    computer.assert(conn, "foundation:Computer", &hostname, "computer", "setup")
        .map_err(|e| format!("Failed to create Computer: {}", e))?;

    let hostname_obj = Object::Literal {
        value: hostname.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    computer.add_property(conn, "foundation:hostname", hostname_obj, "setup")
        .map_err(|e| format!("Failed to add hostname: {}", e))?;

    // Link computer to components
    computer.add_property(conn, "foundation:hasProcessor", Object::Iri("foundation:ThisProcessor".to_string()), "setup")
        .map_err(|e| format!("Failed to link Computer -> Processor: {}", e))?;

    computer.add_property(conn, "foundation:hasMemory", Object::Iri("foundation:ThisMemory".to_string()), "setup")
        .map_err(|e| format!("Failed to link Computer -> Memory: {}", e))?;

    computer.add_property(conn, "foundation:hasOperatingSystem", Object::Iri("foundation:ThisOperatingSystem".to_string()), "setup")
        .map_err(|e| format!("Failed to link Computer -> OperatingSystem: {}", e))?;

    // Find the SoftwareRelease for this version using semantic query
    let version = env!("CARGO_PKG_VERSION").to_string();

    // Query: find SoftwareRelease with versionNumber AND releaseOf FoundationProduct
    let releases = Individual::find_by_class_and_properties(
        conn,
        "foundation:SoftwareRelease",
        &[
            ("foundation:versionNumber", &version),
            ("foundation:releaseOf", "foundation:FoundationProduct"),
        ]
    ).map_err(|e| format!("Failed to query for release: {}", e))?;

    let release_iri = releases.first().ok_or_else(|| {
        format!("SoftwareRelease for FOUNDATION version {} not found in ontology. Please add it to SoftwareRelease.ttl", version)
    })?.clone();

    // Create FOUNDATION Application instance
    let foundation_label = format!("FOUNDATION v{}", version);
    let foundation = Individual::new("foundation:ThisFoundationInstance");
    foundation.assert(conn, "foundation:Application", &foundation_label, "apps", "setup")
        .map_err(|e| format!("Failed to create Application instance: {}", e))?;

    // Link Application to SoftwareRelease
    foundation.add_property(conn, "foundation:installedFrom", Object::Iri(release_iri.clone()), "setup")
        .map_err(|e| format!("Failed to link to SoftwareRelease: {}", e))?;

    // Establish relationships
    computer.add_property(conn, "foundation:hasUser", Object::Iri("foundation:ThisUser".to_string()), "setup")
        .map_err(|e| format!("Failed to link Computer -> User: {}", e))?;
    foundation.add_property(conn, "foundation:runsOn", Object::Iri("foundation:ThisComputer".to_string()), "setup")
        .map_err(|e| format!("Failed to link FOUNDATION -> Computer: {}", e))?;

    let result = SetupResult {
        already_setup: false,
        user: UserInfo {
            iri: "foundation:ThisUser".to_string(),
            name: user_name,
            email,
        },
        computer: ComputerInfo {
            iri: "foundation:ThisComputer".to_string(),
            hostname,
            operating_system: OperatingSystemInfo {
                iri: "foundation:ThisOperatingSystem".to_string(),
                name: os_info.name,
                version: os_info.version,
                kernel: os_info.kernel,
            },
            processor: ProcessorInfo {
                iri: "foundation:ThisProcessor".to_string(),
                model: cpu_info.model,
                cores: cpu_info.cores,
                architecture: cpu_info.architecture,
            },
            memory: MemoryInfo {
                iri: "foundation:ThisMemory".to_string(),
                capacity_gb: memory_info.capacity_gb,
                memory_type: memory_info.memory_type,
            },
        },
        foundation: FoundationInfo {
            iri: "foundation:ThisFoundationInstance".to_string(),
            release: SoftwareReleaseInfo {
                iri: release_iri,
                version_number: version,
                license_type: Some("MIT".to_string()),
            },
        },
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await?;

    // Deserialize the result
    serde_json::from_str(&result_json).map_err(|e| e.to_string())
}

// REMOVED: get_existing_setup function was only used in tests and doesn't match
// the actual production code structure. Real code uses setup__init directly.

/// Internal hardware information structures (without IRI)
#[derive(Debug)]
struct InternalProcessorInfo {
    model: String,
    cores: Option<i64>,
    architecture: String,
}

#[derive(Debug)]
struct InternalMemoryInfo {
    capacity_gb: i64,
    memory_type: String,
}

#[derive(Debug)]
struct InternalOperatingSystemInfo {
    name: String,
    version: String,
    kernel: String,
}

/// Get detailed CPU information
fn get_cpu_info() -> InternalProcessorInfo {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let model = if let Ok(output) = Command::new("sysctl")
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
            model,
            cores,
            architecture,
        };
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;

        let mut model = "Unknown CPU".to_string();
        let mut cores = None;

        if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
            for line in content.lines() {
                if line.starts_with("model name") {
                    if let Some(cpu) = line.split(':').nth(1) {
                        model = cpu.trim().to_string();
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
            model,
            cores,
            architecture,
        };
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let model = if let Ok(output) = Command::new("wmic")
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
            model,
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
fn get_memory_info() -> InternalMemoryInfo {
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

        // Try to detect memory type (DDR3, DDR4, DDR5, LPDDR, etc.)
        let memory_type = "Unknown".to_string(); // macOS doesn't easily expose this

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

        // Try to get memory type from WMIC
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
fn get_os_info() -> InternalOperatingSystemInfo {
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
