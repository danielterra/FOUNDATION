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
#[allow(non_snake_case)]
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
#[allow(non_snake_case)]
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

    // Create Person instance with metadata
    let user = Individual::new("foundation:ThisUser");
    user.assert(&mut conn, "foundation:Person", &user_name, "person", "setup")
        .map_err(|e| format!("Failed to create Person: {}", e))?;
    user.add_string_property(&mut conn, "foundation:name", &user_name, Some("en"), "setup")
        .map_err(|e| format!("Failed to add user name: {}", e))?;
    if let Some(ref email_val) = email {
        user.add_string_property(&mut conn, "foundation:email", email_val, None, "setup")
            .map_err(|e| format!("Failed to add user email: {}", e))?;
    }

    // Create Computer instance with metadata
    let computer = Individual::new("foundation:ThisComputer");
    computer.assert(&mut conn, "foundation:Computer", &hostname, "computer", "setup")
        .map_err(|e| format!("Failed to create Computer: {}", e))?;
    computer.add_string_property(&mut conn, "foundation:hostname", &hostname, None, "setup")
        .map_err(|e| format!("Failed to add hostname: {}", e))?;
    computer.add_string_property(&mut conn, "foundation:os", &os, None, "setup")
        .map_err(|e| format!("Failed to add OS: {}", e))?;
    computer.add_string_property(&mut conn, "foundation:cpu", &cpu, None, "setup")
        .map_err(|e| format!("Failed to add CPU: {}", e))?;
    computer.add_number_property(&mut conn, "foundation:ramGB", ram_gb, "setup")
        .map_err(|e| format!("Failed to add RAM: {}", e))?;

    // Create FOUNDATION instance with metadata
    let version = env!("CARGO_PKG_VERSION").to_string();
    let foundation_label = format!("FOUNDATION v{}", version);
    let foundation = Individual::new("foundation:ThisFoundationInstance");
    foundation.assert(&mut conn, "foundation:FoundationApp", &foundation_label, "apps", "setup")
        .map_err(|e| format!("Failed to create FoundationApp: {}", e))?;
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
            foundation.assert(&mut conn, "foundation:FoundationApp", "Test Foundation", "apps", "test").unwrap();
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

    #[test]
    fn test_get_existing_setup() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        // Setup system first
        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "Jane Doe", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "Jane Doe", Some("en"), "test").unwrap();
        user.add_string_property(&mut conn, "foundation:email", "jane@example.com", None, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "test-host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "test-host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "Test CPU", None, "test").unwrap();
        computer.add_number_property(&mut conn, "foundation:ramGB", 16.0, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION v1.0", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "1.0.0", None, "test").unwrap();

        // Test get_existing_setup
        let result = get_existing_setup(&conn);
        assert!(result.is_ok());

        let setup = result.unwrap();
        assert!(setup.already_setup);
        assert_eq!(setup.user.name, "Jane Doe");
        assert_eq!(setup.user.email, Some("jane@example.com".to_string()));
        assert_eq!(setup.computer.hostname, "test-host");
        assert_eq!(setup.computer.os, "linux");
        assert_eq!(setup.computer.cpu, "Test CPU");
        assert_eq!(setup.computer.ram_gb, 16.0);
        assert_eq!(setup.foundation.version, "1.0.0");
    }

    #[test]
    fn test_get_existing_setup_without_email() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        // Setup without email
        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "John", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "John", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "macos", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "M1", None, "test").unwrap();
        computer.add_number_property(&mut conn, "foundation:ramGB", 8.0, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_ok());

        let setup = result.unwrap();
        assert_eq!(setup.user.email, None);
    }

    #[test]
    fn test_get_existing_setup_missing_user() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        // Setup without user data
        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "CPU", None, "test").unwrap();
        computer.add_number_property(&mut conn, "foundation:ramGB", 8.0, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("User name not found"));
    }

    #[test]
    fn test_get_existing_setup_missing_computer() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        // Setup without computer hostname
        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "User", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        // Missing hostname
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "CPU", None, "test").unwrap();
        computer.add_number_property(&mut conn, "foundation:ramGB", 8.0, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Hostname not found"));
    }

    #[test]
    fn test_get_cpu_info_returns_string() {
        let cpu = get_cpu_info();
        assert!(!cpu.is_empty());
        // Should either return actual CPU info or "Unknown CPU"
        assert!(cpu.len() > 0);
    }

    #[test]
    fn test_get_ram_gb_returns_positive() {
        let ram = get_ram_gb();
        // Should return positive value or 0.0 if detection fails
        assert!(ram >= 0.0);
    }

    #[test]
    fn test_setup_creates_relationships() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__init(
            "Test User".to_string(),
            None,
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());

        // Verify relationships exist
        let state = app.state::<Mutex<Connection>>();
        let conn = state.lock().unwrap();
        let computer = Individual::new("foundation:ThisComputer");
        let has_user = computer.get_property_values(&conn, "foundation:hasUser").unwrap();
        assert!(!has_user.is_empty());

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        let runs_on = foundation.get_property_values(&conn, "foundation:runsOn").unwrap();
        assert!(!runs_on.is_empty());
    }

    #[test]
    fn test_setup_with_special_characters_in_name() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__init(
            "José María O'Brien".to_string(),
            Some("josé@example.com".to_string()),
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let setup_result = result.unwrap();
        assert_eq!(setup_result.user.name, "José María O'Brien");
        assert_eq!(setup_result.user.email, Some("josé@example.com".to_string()));
    }

    #[test]
    fn test_setup_result_iris() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__init(
            "Test".to_string(),
            None,
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let setup_result = result.unwrap();
        assert_eq!(setup_result.user.iri, "foundation:ThisUser");
        assert_eq!(setup_result.computer.iri, "foundation:ThisComputer");
        assert_eq!(setup_result.foundation.iri, "foundation:ThisFoundationInstance");
    }

    #[test]
    fn test_get_existing_setup_missing_os() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "User", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        // Missing OS
        computer.add_string_property(&mut conn, "foundation:cpu", "CPU", None, "test").unwrap();
        computer.add_number_property(&mut conn, "foundation:ramGB", 8.0, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("OS not found"));
    }

    #[test]
    fn test_get_existing_setup_missing_cpu() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "User", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        // Missing CPU
        computer.add_number_property(&mut conn, "foundation:ramGB", 8.0, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("CPU not found"));
    }

    #[test]
    fn test_get_existing_setup_missing_ram() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "User", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "CPU", None, "test").unwrap();
        // Missing RAM

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("RAM not found"));
    }

    #[test]
    fn test_get_existing_setup_missing_version() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "User", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "CPU", None, "test").unwrap();
        computer.add_number_property(&mut conn, "foundation:ramGB", 8.0, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        // Missing version

        let result = get_existing_setup(&conn);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Version not found"));
    }

    #[test]
    fn test_get_existing_setup_ram_as_integer() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "User", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "CPU", None, "test").unwrap();
        // Add RAM as integer instead of float
        computer.add_integer_property(&mut conn, "foundation:ramGB", 16, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_ok());
        let setup = result.unwrap();
        assert_eq!(setup.computer.ram_gb, 16.0);
    }

    #[test]
    fn test_setup_check_after_init() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        // Check before setup
        let check_before = setup__check(app.state::<Mutex<Connection>>());
        assert!(check_before.is_ok());
        assert!(!check_before.unwrap());

        // Run setup
        let setup_result = setup__init(
            "Test User".to_string(),
            Some("test@test.com".to_string()),
            app.state::<Mutex<Connection>>(),
        );
        assert!(setup_result.is_ok());

        // Check after setup
        let check_after = setup__check(app.state::<Mutex<Connection>>());
        assert!(check_after.is_ok());
        assert!(check_after.unwrap());
    }

    #[test]
    fn test_setup_data_persistence() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        // Run setup
        let setup_result = setup__init(
            "Persistent User".to_string(),
            Some("persist@example.com".to_string()),
            app.state::<Mutex<Connection>>(),
        );
        assert!(setup_result.is_ok());
        let original = setup_result.unwrap();

        // Verify data persisted by reading it back
        let state = app.state::<Mutex<Connection>>();
        let conn = state.lock().unwrap();
        let retrieved = get_existing_setup(&conn);
        assert!(retrieved.is_ok());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.user.name, "Persistent User");
        assert_eq!(retrieved.user.email, Some("persist@example.com".to_string()));
        assert_eq!(retrieved.computer.hostname, original.computer.hostname);
        assert_eq!(retrieved.computer.os, original.computer.os);
        assert_eq!(retrieved.computer.cpu, original.computer.cpu);
        assert_eq!(retrieved.computer.ram_gb, original.computer.ram_gb);
        assert_eq!(retrieved.foundation.version, original.foundation.version);
    }

    #[test]
    fn test_setup_computer_properties_complete() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__init(
            "Test".to_string(),
            None,
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let setup = result.unwrap();

        // Verify all computer properties are non-empty
        assert!(!setup.computer.hostname.is_empty());
        assert!(!setup.computer.os.is_empty());
        assert!(!setup.computer.cpu.is_empty());
        assert!(setup.computer.ram_gb >= 0.0);
    }

    #[test]
    fn test_setup_foundation_version_matches() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__init(
            "Test".to_string(),
            None,
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let setup = result.unwrap();

        // Verify version matches package version
        let expected_version = env!("CARGO_PKG_VERSION");
        assert_eq!(setup.foundation.version, expected_version);
    }

    #[test]
    fn test_get_existing_setup_ram_as_string() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "User", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "CPU", None, "test").unwrap();
        // Add RAM as string that can be parsed
        computer.add_string_property(&mut conn, "foundation:ramGB", "32.5", None, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_ok());
        let setup = result.unwrap();
        assert_eq!(setup.computer.ram_gb, 32.5);
    }

    #[test]
    fn test_get_existing_setup_ram_invalid_string() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "User", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "host", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "linux", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "CPU", None, "test").unwrap();
        // Add RAM as string that cannot be parsed
        computer.add_string_property(&mut conn, "foundation:ramGB", "invalid", None, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "0.1.0", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("RAM not found"));
    }

    #[test]
    fn test_setup_check_returns_false_initially() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__check(app.state::<Mutex<Connection>>());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn test_setup_check_returns_true_after_setup() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        // Create foundation instance
        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__check(app.state::<Mutex<Connection>>());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_user_info_struct() {
        let user_info = UserInfo {
            iri: "test:User".to_string(),
            name: "Test User".to_string(),
            email: Some("test@example.com".to_string()),
        };

        assert_eq!(user_info.iri, "test:User");
        assert_eq!(user_info.name, "Test User");
        assert_eq!(user_info.email, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_computer_info_struct() {
        let computer_info = ComputerInfo {
            iri: "test:Computer".to_string(),
            hostname: "test-host".to_string(),
            os: "linux".to_string(),
            cpu: "Test CPU".to_string(),
            ram_gb: 16.0,
        };

        assert_eq!(computer_info.iri, "test:Computer");
        assert_eq!(computer_info.hostname, "test-host");
        assert_eq!(computer_info.os, "linux");
        assert_eq!(computer_info.cpu, "Test CPU");
        assert_eq!(computer_info.ram_gb, 16.0);
    }

    #[test]
    fn test_foundation_info_struct() {
        let foundation_info = FoundationInfo {
            iri: "test:Foundation".to_string(),
            version: "1.0.0".to_string(),
        };

        assert_eq!(foundation_info.iri, "test:Foundation");
        assert_eq!(foundation_info.version, "1.0.0");
    }

    #[test]
    fn test_setup_result_struct() {
        let setup_result = SetupResult {
            already_setup: true,
            user: UserInfo {
                iri: "test:User".to_string(),
                name: "Test".to_string(),
                email: None,
            },
            computer: ComputerInfo {
                iri: "test:Computer".to_string(),
                hostname: "host".to_string(),
                os: "linux".to_string(),
                cpu: "CPU".to_string(),
                ram_gb: 8.0,
            },
            foundation: FoundationInfo {
                iri: "test:Foundation".to_string(),
                version: "1.0".to_string(),
            },
        };

        assert!(setup_result.already_setup);
        assert_eq!(setup_result.user.name, "Test");
        assert_eq!(setup_result.computer.hostname, "host");
        assert_eq!(setup_result.foundation.version, "1.0");
    }

    #[test]
    fn test_setup_with_empty_name() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let result = setup__init(
            "".to_string(),
            None,
            app.state::<Mutex<Connection>>(),
        );

        // Should succeed even with empty name (validation is UI responsibility)
        assert!(result.is_ok());
        let setup = result.unwrap();
        assert_eq!(setup.user.name, "");
    }

    #[test]
    fn test_setup_with_very_long_name() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let long_name = "A".repeat(1000);
        let result = setup__init(
            long_name.clone(),
            None,
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let setup = result.unwrap();
        assert_eq!(setup.user.name, long_name);
    }

    #[test]
    fn test_setup_with_very_long_email() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let app = tauri::test::mock_app();
        app.manage(Mutex::new(conn));

        let long_email = format!("{}@example.com", "a".repeat(500));
        let result = setup__init(
            "User".to_string(),
            Some(long_email.clone()),
            app.state::<Mutex<Connection>>(),
        );

        assert!(result.is_ok());
        let setup = result.unwrap();
        assert_eq!(setup.user.email, Some(long_email));
    }

    #[test]
    fn test_get_existing_setup_with_empty_values() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "", "person", "test").unwrap();
        user.add_string_property(&mut conn, "foundation:name", "", Some("en"), "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "", "computer", "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:hostname", "", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:os", "", None, "test").unwrap();
        computer.add_string_property(&mut conn, "foundation:cpu", "", None, "test").unwrap();
        computer.add_number_property(&mut conn, "foundation:ramGB", 0.0, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "", "apps", "test").unwrap();
        foundation.add_string_property(&mut conn, "foundation:version", "", None, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_ok());
        let setup = result.unwrap();
        assert_eq!(setup.user.name, "");
        assert_eq!(setup.computer.hostname, "");
        assert_eq!(setup.computer.ram_gb, 0.0);
    }
}
