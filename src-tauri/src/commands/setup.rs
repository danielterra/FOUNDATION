use serde::Serialize;
use tauri::State;
use rusqlite::Connection;
use std::sync::Mutex;

use crate::owl::Individual;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub already_setup: bool,
    pub user: UserInfo,
    pub computer: ComputerInfo,
    pub foundation: FoundationInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub iri: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerInfo {
    pub iri: String,
    pub hostname: String,
    pub os: String,
    pub cpu: String,
    pub ram_gb: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundationInfo {
    pub iri: String,
    pub version: String,
}

/// Check if setup has been completed
#[tauri::command]
pub fn setup__check(
    conn: State<'_, Mutex<Connection>>,
) -> Result<bool, String> {
    let conn = conn.lock().map_err(|e| e.to_string())?;
    let foundation_instance = Individual::new("foundation:ThisFoundationInstance");
    foundation_instance.exists(&conn)
        .map_err(|e| format!("Failed to check setup status: {}", e))
}

/// Initialize setup: detect system, create instances, establish relationships
/// Should only be called when setup__check returns false
#[tauri::command]
pub fn setup__init(
    user_name: String,
    email: Option<String>,
    conn: State<'_, Mutex<Connection>>,
) -> Result<SetupResult, String> {
    let mut conn = conn.lock().map_err(|e| e.to_string())?;

    // Don't check again - assume caller used setup__check first

    // Detect system information
    let hostname = hostname::get()
        .map_err(|e| format!("Failed to get hostname: {}", e))?
        .to_string_lossy()
        .to_string();

    let os = std::env::consts::OS.to_string();
    let cpu = get_cpu_info();
    let ram_gb = get_ram_gb();

    // Create Person instance
    let user = Individual::new("foundation:ThisUser");
    user.assert_type(&mut conn, "foundation:Person", "setup")
        .map_err(|e| format!("Failed to create Person: {}", e))?;
    user.add_string_property(&mut conn, "foundation:name", &user_name, Some("en"), "setup")
        .map_err(|e| format!("Failed to add user name: {}", e))?;
    if let Some(ref email_val) = email {
        user.add_string_property(&mut conn, "foundation:email", email_val, None, "setup")
            .map_err(|e| format!("Failed to add user email: {}", e))?;
    }

    // Create Computer instance
    let computer = Individual::new("foundation:ThisComputer");
    computer.assert_type(&mut conn, "foundation:Computer", "setup")
        .map_err(|e| format!("Failed to create Computer: {}", e))?;
    computer.add_string_property(&mut conn, "foundation:hostname", &hostname, None, "setup")
        .map_err(|e| format!("Failed to add hostname: {}", e))?;
    computer.add_string_property(&mut conn, "foundation:os", &os, None, "setup")
        .map_err(|e| format!("Failed to add OS: {}", e))?;
    computer.add_string_property(&mut conn, "foundation:cpu", &cpu, None, "setup")
        .map_err(|e| format!("Failed to add CPU: {}", e))?;
    computer.add_number_property(&mut conn, "foundation:ramGB", ram_gb, "setup")
        .map_err(|e| format!("Failed to add RAM: {}", e))?;

    // Create FOUNDATION instance
    let foundation = Individual::new("foundation:ThisFoundationInstance");
    foundation.assert_type(&mut conn, "foundation:FoundationApp", "setup")
        .map_err(|e| format!("Failed to create FoundationApp: {}", e))?;
    let version = env!("CARGO_PKG_VERSION").to_string();
    foundation.add_string_property(&mut conn, "foundation:version", &version, None, "setup")
        .map_err(|e| format!("Failed to add version: {}", e))?;

    // Establish relationships
    computer.add_object_property(&mut conn, "foundation:hasUser", "foundation:ThisUser", "setup")
        .map_err(|e| format!("Failed to link Computer -> User: {}", e))?;
    foundation.add_object_property(&mut conn, "foundation:runsOn", "foundation:ThisComputer", "setup")
        .map_err(|e| format!("Failed to link FOUNDATION -> Computer: {}", e))?;

    Ok(SetupResult {
        already_setup: false,
        user: UserInfo {
            iri: "foundation:ThisUser".to_string(),
            name: user_name,
            email,
        },
        computer: ComputerInfo {
            iri: "foundation:ThisComputer".to_string(),
            hostname,
            os,
            cpu,
            ram_gb,
        },
        foundation: FoundationInfo {
            iri: "foundation:ThisFoundationInstance".to_string(),
            version,
        },
    })
}

/// Get existing setup data when setup is already done
fn get_existing_setup(conn: &Connection) -> Result<SetupResult, String> {
    let user = Individual::new("foundation:ThisUser");
    let computer = Individual::new("foundation:ThisComputer");
    let foundation = Individual::new("foundation:ThisFoundationInstance");

    // Get user data
    let name_values = user.get_property_values(conn, "foundation:name")
        .map_err(|e| format!("Failed to get user name: {}", e))?;
    let name = name_values.first()
        .and_then(|v| v.as_literal())
        .ok_or("User name not found")?;

    let email_values = user.get_property_values(conn, "foundation:email")
        .map_err(|e| format!("Failed to get user email: {}", e))?;
    let email = email_values.first().and_then(|v| v.as_literal());

    // Get computer data
    let hostname = computer.get_property_values(conn, "foundation:hostname")
        .map_err(|e| format!("Failed to get hostname: {}", e))?
        .first()
        .and_then(|v| v.as_literal())
        .ok_or("Hostname not found")?;

    let os = computer.get_property_values(conn, "foundation:os")
        .map_err(|e| format!("Failed to get OS: {}", e))?
        .first()
        .and_then(|v| v.as_literal())
        .ok_or("OS not found")?;

    let cpu = computer.get_property_values(conn, "foundation:cpu")
        .map_err(|e| format!("Failed to get CPU: {}", e))?
        .first()
        .and_then(|v| v.as_literal())
        .ok_or("CPU not found")?;

    let ram_gb = computer.get_property_values(conn, "foundation:ramGB")
        .map_err(|e| format!("Failed to get RAM: {}", e))?
        .first()
        .and_then(|v| match v {
            crate::eavto::Object::Number(n) => Some(*n),
            crate::eavto::Object::Integer(i) => Some(*i as f64),
            crate::eavto::Object::Literal { value, .. } => value.parse::<f64>().ok(),
            _ => None,
        })
        .ok_or("RAM not found")?;

    // Get foundation data
    let version = foundation.get_property_values(conn, "foundation:version")
        .map_err(|e| format!("Failed to get version: {}", e))?
        .first()
        .and_then(|v| v.as_literal())
        .ok_or("Version not found")?;

    Ok(SetupResult {
        already_setup: true,
        user: UserInfo {
            iri: "foundation:ThisUser".to_string(),
            name,
            email,
        },
        computer: ComputerInfo {
            iri: "foundation:ThisComputer".to_string(),
            hostname,
            os,
            cpu,
            ram_gb,
        },
        foundation: FoundationInfo {
            iri: "foundation:ThisFoundationInstance".to_string(),
            version,
        },
    })
}

/// Get CPU information
fn get_cpu_info() -> String {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("sysctl")
            .args(&["-n", "machdep.cpu.brand_string"])
            .output()
        {
            if let Ok(cpu) = String::from_utf8(output.stdout) {
                return cpu.trim().to_string();
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;
        if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
            for line in content.lines() {
                if line.starts_with("model name") {
                    if let Some(cpu) = line.split(':').nth(1) {
                        return cpu.trim().to_string();
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("wmic")
            .args(&["cpu", "get", "name"])
            .output()
        {
            if let Ok(cpu) = String::from_utf8(output.stdout) {
                let lines: Vec<&str> = cpu.lines().collect();
                if lines.len() > 1 {
                    return lines[1].trim().to_string();
                }
            }
        }
    }

    "Unknown CPU".to_string()
}

/// Get RAM in GB
fn get_ram_gb() -> f64 {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("sysctl")
            .args(&["-n", "hw.memsize"])
            .output()
        {
            if let Ok(ram_str) = String::from_utf8(output.stdout) {
                if let Ok(ram_bytes) = ram_str.trim().parse::<u64>() {
                    return ram_bytes as f64 / 1_073_741_824.0; // Convert bytes to GB
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;
        if let Ok(content) = fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(ram_kb) = line.split_whitespace().nth(1) {
                        if let Ok(ram_kb) = ram_kb.parse::<u64>() {
                            return ram_kb as f64 / 1_048_576.0; // Convert KB to GB
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("wmic")
            .args(&["computersystem", "get", "totalphysicalmemory"])
            .output()
        {
            if let Ok(ram_str) = String::from_utf8(output.stdout) {
                let lines: Vec<&str> = ram_str.lines().collect();
                if lines.len() > 1 {
                    if let Ok(ram_bytes) = lines[1].trim().parse::<u64>() {
                        return ram_bytes as f64 / 1_073_741_824.0; // Convert bytes to GB
                    }
                }
            }
        }
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eavto::test_helpers::setup_test_db;
    use crate::owl::Class;
    use tauri::Manager;

    fn setup_test_ontology(conn: &mut Connection) {
        use crate::owl::ClassType;

        // Create necessary classes
        let person = Class::new("foundation:Person");
        person.assert_class(conn, ClassType::OwlClass, "test").unwrap();
        person.add_super_class(conn, "owl:Thing", "test").unwrap();

        let computer = Class::new("foundation:Computer");
        computer.assert_class(conn, ClassType::OwlClass, "test").unwrap();
        computer.add_super_class(conn, "owl:Thing", "test").unwrap();

        let foundation_app = Class::new("foundation:FoundationApp");
        foundation_app.assert_class(conn, ClassType::OwlClass, "test").unwrap();
        foundation_app.add_super_class(conn, "owl:Thing", "test").unwrap();
    }

    #[test]
    fn test_setup_first_time() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__init(
            "John Doe".to_string(),
            Some("john@example.com".to_string()),
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let setup_result = result.unwrap();
        assert!(!setup_result.already_setup);
        assert_eq!(setup_result.user.name, "John Doe");
        assert_eq!(setup_result.user.email, Some("john@example.com".to_string()));
        assert!(!setup_result.computer.hostname.is_empty());
        assert!(!setup_result.foundation.version.is_empty());
    }

    #[test]
    fn test_setup_check() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        // Check before setup - should be false
        let result = setup__check(app.state::<Mutex<Connection>>());
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Create setup
        {
            let state = app.state::<Mutex<Connection>>();
            let mut conn = state.lock().unwrap();
            let foundation = Individual::new("foundation:ThisFoundationInstance");
            foundation.assert_type(&mut conn, "foundation:FoundationApp", "test").unwrap();
        }

        // Check after setup - should be true
        let result = setup__check(app.state::<Mutex<Connection>>());
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_setup_without_email() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__init(
            "Bob Smith".to_string(),
            None,
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let setup_result = result.unwrap();
        assert!(!setup_result.already_setup);
        assert_eq!(setup_result.user.name, "Bob Smith");
        assert_eq!(setup_result.user.email, None);
    }
}
