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
pub struct ComputerInfo {
    pub iri: String,
    pub hostname: String,
    pub os: String,
    pub cpu: String,
    pub ram_gb: f64,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundationInfo {
    pub iri: String,
    pub version: String,
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

    let os = std::env::consts::OS.to_string();
    let cpu = get_cpu_info();
    let ram_gb = get_ram_gb();

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

    let os_obj = Object::Literal {
        value: os.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    computer.add_property(conn, "foundation:os", os_obj, "setup")
        .map_err(|e| format!("Failed to add OS: {}", e))?;

    let cpu_obj = Object::Literal {
        value: cpu.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    computer.add_property(conn, "foundation:cpu", cpu_obj, "setup")
        .map_err(|e| format!("Failed to add CPU: {}", e))?;

    computer.add_property(conn, "foundation:hasMemory", Object::Integer(ram_gb as i64), "setup")
        .map_err(|e| format!("Failed to add RAM: {}", e))?;

    // Create FOUNDATION instance with metadata
    let version = env!("CARGO_PKG_VERSION").to_string();
    let foundation_label = format!("FOUNDATION v{}", version);
    let foundation = Individual::new("foundation:ThisFoundationInstance");
    foundation.assert(conn, "foundation:FoundationApp", &foundation_label, "apps", "setup")
        .map_err(|e| format!("Failed to create FoundationApp: {}", e))?;

    let version_obj = Object::Literal {
        value: version.clone(),
        datatype: Some("xsd:string".to_string()),
        language: None,
    };
    foundation.add_property(conn, "foundation:version", version_obj, "setup")
        .map_err(|e| format!("Failed to add version: {}", e))?;

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
            os,
            cpu,
            ram_gb,
        },
        foundation: FoundationInfo {
            iri: "foundation:ThisFoundationInstance".to_string(),
            version,
        },
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
    }).await?;

    // Deserialize the result
    serde_json::from_str(&result_json).map_err(|e| e.to_string())
}

/// Get existing setup data when setup is already done
fn get_existing_setup(conn: &Connection) -> Result<SetupResult, String> {
    let user = Individual::get(conn, "foundation:ThisUser")
        .map_err(|e| format!("Failed to get user: {}", e))?;
    let computer = Individual::get(conn, "foundation:ThisComputer")
        .map_err(|e| format!("Failed to get computer: {}", e))?;
    let foundation = Individual::get(conn, "foundation:ThisFoundationInstance")
        .map_err(|e| format!("Failed to get foundation: {}", e))?;

    // Get user data
    let name = user.properties.iter()
        .find(|(prop, _)| prop == "foundation:name")
        .and_then(|(_, val)| val.as_literal())
        .ok_or("User name not found")?;

    let email = user.properties.iter()
        .find(|(prop, _)| prop == "foundation:email")
        .and_then(|(_, val)| val.as_literal());

    // Get computer data
    let hostname = computer.properties.iter()
        .find(|(prop, _)| prop == "foundation:hostname")
        .and_then(|(_, val)| val.as_literal())
        .ok_or("Hostname not found")?;

    let os = computer.properties.iter()
        .find(|(prop, _)| prop == "foundation:os")
        .and_then(|(_, val)| val.as_literal())
        .ok_or("OS not found")?;

    let cpu = computer.properties.iter()
        .find(|(prop, _)| prop == "foundation:cpu")
        .and_then(|(_, val)| val.as_literal())
        .ok_or("CPU not found")?;

    let ram_gb = computer.properties.iter()
        .find(|(prop, _)| prop == "foundation:hasMemory")
        .and_then(|(_, val)| match val {
            Object::Number(n) => Some(*n),
            Object::Integer(i) => Some(*i as f64),
            Object::Literal { value, .. } => value.parse::<f64>().ok(),
            _ => None,
        })
        .ok_or("RAM not found")?;

    // Get foundation data
    let version = foundation.properties.iter()
        .find(|(prop, _)| prop == "foundation:version")
        .and_then(|(_, val)| val.as_literal())
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
    use crate::eavto::executor::DbExecutor;
    use crate::owl::Class;
    use tauri::Manager;

    // Helper to run async tests synchronously
    fn run_async<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    fn setup_test_ontology(conn: &mut Connection) {
        use crate::owl::{ClassType, Property, PropertyType};

        // Create necessary classes
        let person = Class::new("foundation:Person");
        person.assert(conn, ClassType::OwlClass, "Person", "person", Some("owl:Thing"), "test").unwrap();

        let computer = Class::new("foundation:Computer");
        computer.assert(conn, ClassType::OwlClass, "Computer", "computer", Some("owl:Thing"), "test").unwrap();

        let foundation_app = Class::new("foundation:FoundationApp");
        foundation_app.assert(conn, ClassType::OwlClass, "Foundation App", "apps", Some("owl:Thing"), "test").unwrap();

        // Define properties for Person class
        Property::new("foundation:name").assert(
            conn, PropertyType::DatatypeProperty, "name", None, Some("foundation:Person"), None, None, "test"
        ).unwrap();
        Property::new("foundation:email").assert(
            conn, PropertyType::DatatypeProperty, "email", None, Some("foundation:Person"), None, None, "test"
        ).unwrap();

        // Define properties for Computer class
        Property::new("foundation:hostname").assert(
            conn, PropertyType::DatatypeProperty, "hostname", None, Some("foundation:Computer"), None, None, "test"
        ).unwrap();
        Property::new("foundation:os").assert(
            conn, PropertyType::DatatypeProperty, "os", None, Some("foundation:Computer"), None, None, "test"
        ).unwrap();
        Property::new("foundation:cpu").assert(
            conn, PropertyType::DatatypeProperty, "cpu", None, Some("foundation:Computer"), None, None, "test"
        ).unwrap();
        Property::new("foundation:hasMemory").assert(
            conn, PropertyType::DatatypeProperty, "has memory", None, Some("foundation:Computer"), Some("xsd:integer"), Some("unit:GigaBYTE"), "test"
        ).unwrap();
        Property::new("foundation:hasUser").assert(
            conn, PropertyType::ObjectProperty, "hasUser", None, Some("foundation:Computer"), Some("foundation:Person"), None, "test"
        ).unwrap();

        // Define properties for FoundationApp class
        Property::new("foundation:version").assert(
            conn, PropertyType::DatatypeProperty, "version", None, Some("foundation:FoundationApp"), None, None, "test"
        ).unwrap();
        Property::new("foundation:runsOn").assert(
            conn, PropertyType::ObjectProperty, "runsOn", None, Some("foundation:FoundationApp"), Some("foundation:Computer"), None, "test"
        ).unwrap();
    }

    #[test]
    fn test_setup_first_time() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__init(
                "John Doe".to_string(),
                Some("john@example.com".to_string()),
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());
            let setup_result = result.unwrap();
            assert!(!setup_result.already_setup);
            assert_eq!(setup_result.user.name, "John Doe");
            assert_eq!(setup_result.user.email, Some("john@example.com".to_string()));
            assert!(!setup_result.computer.hostname.is_empty());
            assert!(!setup_result.foundation.version.is_empty());
        });
    }

    #[test]
    fn test_setup_check() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            // Check before setup - should be false
            let result = setup__check(app.state::<DbExecutor>()).await;
            assert!(result.is_ok());
            assert!(!result.unwrap());

            // Create setup
            let executor = app.state::<DbExecutor>();
            executor.write(|conn| {
                let foundation = Individual::new("foundation:ThisFoundationInstance");
                foundation.assert(conn, "foundation:FoundationApp", "Test Foundation", "apps", "test")
                    .map_err(|e| e.to_string())?;
                Ok("".to_string())
            }).await.unwrap();

            // Check after setup - should be true
            let result = setup__check(app.state::<DbExecutor>()).await;
            assert!(result.is_ok());
            assert!(result.unwrap());
        });
    }

    #[test]
    fn test_setup_without_email() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__init(
                "Bob Smith".to_string(),
                None,
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());
            let setup_result = result.unwrap();
            assert!(!setup_result.already_setup);
            assert_eq!(setup_result.user.name, "Bob Smith");
            assert_eq!(setup_result.user.email, None);
        });
    }

    #[test]
    fn test_get_existing_setup() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        // Setup system first
        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "Jane Doe", "person", "test").unwrap();
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "Jane Doe".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();
        user.add_property(&mut conn, "foundation:email", Object::Literal {
            value: "jane@example.com".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "test-host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "test-host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "Test CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(16), "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION v1.0", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "1.0.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

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
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "John".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "macos".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "M1".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(8), "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

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
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(8), "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

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
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "User".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        // Missing hostname
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(8), "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

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
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__init(
                "Test User".to_string(),
                None,
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());

            // Verify relationships exist
            let executor = app.state::<DbExecutor>();
            executor.read(|conn| {
                let computer = Individual::get(conn, "foundation:ThisComputer").unwrap();
                let has_user: Vec<_> = computer.properties.iter()
                    .filter(|(prop, _)| prop == "foundation:hasUser")
                    .collect();
                assert!(!has_user.is_empty());

                let foundation = Individual::get(conn, "foundation:ThisFoundationInstance").unwrap();
                let runs_on: Vec<_> = foundation.properties.iter()
                    .filter(|(prop, _)| prop == "foundation:runsOn")
                    .collect();
                assert!(!runs_on.is_empty());
                Ok(())
            }).await.unwrap();
        });
    }

    #[test]
    fn test_setup_with_special_characters_in_name() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__init(
                "José María O'Brien".to_string(),
                Some("josé@example.com".to_string()),
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());
            let setup_result = result.unwrap();
            assert_eq!(setup_result.user.name, "José María O'Brien");
            assert_eq!(setup_result.user.email, Some("josé@example.com".to_string()));
        });
    }

    #[test]
    fn test_setup_result_iris() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__init(
                "Test".to_string(),
                None,
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());
            let setup_result = result.unwrap();
            assert_eq!(setup_result.user.iri, "foundation:ThisUser");
            assert_eq!(setup_result.computer.iri, "foundation:ThisComputer");
            assert_eq!(setup_result.foundation.iri, "foundation:ThisFoundationInstance");
        });
    }

    #[test]
    fn test_get_existing_setup_missing_os() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "User".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        // Missing OS
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(8), "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

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
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "User".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        // Missing CPU
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(8), "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

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
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "User".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        // Missing RAM

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

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
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "User".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(8), "test").unwrap();

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
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "User".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        // Add RAM as integer instead of float
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(16), "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_ok());
        let setup = result.unwrap();
        assert_eq!(setup.computer.ram_gb, 16.0);
    }

    #[test]
    fn test_setup_check_after_init() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            // Check before setup
            let check_before = setup__check(app.state::<DbExecutor>()).await;
            assert!(check_before.is_ok());
            assert!(!check_before.unwrap());

            // Run setup
            let setup_result = setup__init(
                "Test User".to_string(),
                Some("test@test.com".to_string()),
                app.state::<DbExecutor>(),
            ).await;
            assert!(setup_result.is_ok());

            // Check after setup
            let check_after = setup__check(app.state::<DbExecutor>()).await;
            assert!(check_after.is_ok());
            assert!(check_after.unwrap());
        });
    }

    #[test]
    fn test_setup_data_persistence() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            // Run setup
            let setup_result = setup__init(
                "Persistent User".to_string(),
                Some("persist@example.com".to_string()),
                app.state::<DbExecutor>(),
            ).await;
            assert!(setup_result.is_ok());
            let original = setup_result.unwrap();

            // Verify data persisted by reading it back
            let executor = app.state::<DbExecutor>();
            executor.read(move |conn| {
                let retrieved = get_existing_setup(conn);
                assert!(retrieved.is_ok());

                let retrieved = retrieved.unwrap();
                assert_eq!(retrieved.user.name, "Persistent User");
                assert_eq!(retrieved.user.email, Some("persist@example.com".to_string()));
                assert_eq!(retrieved.computer.hostname, original.computer.hostname);
                assert_eq!(retrieved.computer.os, original.computer.os);
                assert_eq!(retrieved.computer.cpu, original.computer.cpu);
                assert_eq!(retrieved.computer.ram_gb, original.computer.ram_gb);
                assert_eq!(retrieved.foundation.version, original.foundation.version);
                Ok(())
            }).await.unwrap();
        });
    }

    #[test]
    fn test_setup_computer_properties_complete() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__init(
                "Test".to_string(),
                None,
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());
            let setup = result.unwrap();

            // Verify all computer properties are non-empty
            assert!(!setup.computer.hostname.is_empty());
            assert!(!setup.computer.os.is_empty());
            assert!(!setup.computer.cpu.is_empty());
            assert!(setup.computer.ram_gb >= 0.0);
        });
    }

    #[test]
    fn test_setup_foundation_version_matches() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__init(
                "Test".to_string(),
                None,
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());
            let setup = result.unwrap();

            // Verify version matches package version
            let expected_version = env!("CARGO_PKG_VERSION");
            assert_eq!(setup.foundation.version, expected_version);
        });
    }

    #[test]
    fn test_get_existing_setup_ram_as_string() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "User", "person", "test").unwrap();
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "User".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        // Add RAM as string that can be parsed
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Literal {
            value: "32.5".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

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
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "User".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "host", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "host".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "linux".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "CPU".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        // Add RAM as string that cannot be parsed
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Literal {
            value: "invalid".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "0.1.0".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("RAM not found"));
    }

    #[test]
    fn test_setup_check_returns_false_initially() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__check(app.state::<DbExecutor>()).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), false);
        });
    }

    #[test]
    fn test_setup_check_returns_true_after_setup() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            // Create foundation instance
            let foundation = Individual::new("foundation:ThisFoundationInstance");
            foundation.assert(&mut conn, "foundation:FoundationApp", "FOUNDATION", "apps", "test").unwrap();

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__check(app.state::<DbExecutor>()).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), true);
        });
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
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let result = setup__init(
                "".to_string(),
                None,
                app.state::<DbExecutor>(),
            ).await;

            // Should succeed even with empty name (validation is UI responsibility)
            assert!(result.is_ok());
            let setup = result.unwrap();
            assert_eq!(setup.user.name, "");
        });
    }

    #[test]
    fn test_setup_with_very_long_name() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let long_name = "A".repeat(1000);
            let result = setup__init(
                long_name.clone(),
                None,
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());
            let setup = result.unwrap();
            assert_eq!(setup.user.name, long_name);
        });
    }

    #[test]
    fn test_setup_with_very_long_email() {
        run_async(async {
            let mut conn = setup_test_db();
            setup_test_ontology(&mut conn);

            let app = tauri::test::mock_app();
            let executor = DbExecutor::new(conn);
            app.manage(executor);

            let long_email = format!("{}@example.com", "a".repeat(500));
            let result = setup__init(
                "User".to_string(),
                Some(long_email.clone()),
                app.state::<DbExecutor>(),
            ).await;

            assert!(result.is_ok());
            let setup = result.unwrap();
            assert_eq!(setup.user.email, Some(long_email));
        });
    }

    #[test]
    fn test_get_existing_setup_with_empty_values() {
        let mut conn = setup_test_db();
        setup_test_ontology(&mut conn);

        let user = Individual::new("foundation:ThisUser");
        user.assert(&mut conn, "foundation:Person", "", "person", "test").unwrap();
        user.add_property(&mut conn, "foundation:name", Object::Literal {
            value: "".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: Some("en".to_string()),
        }, "test").unwrap();

        let computer = Individual::new("foundation:ThisComputer");
        computer.assert(&mut conn, "foundation:Computer", "", "computer", "test").unwrap();
        computer.add_property(&mut conn, "foundation:hostname", Object::Literal {
            value: "".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:os", Object::Literal {
            value: "".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:cpu", Object::Literal {
            value: "".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();
        computer.add_property(&mut conn, "foundation:hasMemory", Object::Integer(0), "test").unwrap();

        let foundation = Individual::new("foundation:ThisFoundationInstance");
        foundation.assert(&mut conn, "foundation:FoundationApp", "", "apps", "test").unwrap();
        foundation.add_property(&mut conn, "foundation:version", Object::Literal {
            value: "".to_string(),
            datatype: Some("xsd:string".to_string()),
            language: None,
        }, "test").unwrap();

        let result = get_existing_setup(&conn);
        assert!(result.is_ok());
        let setup = result.unwrap();
        assert_eq!(setup.user.name, "");
        assert_eq!(setup.computer.hostname, "");
        assert_eq!(setup.computer.ram_gb, 0.0);
    }
}
