// Commands module
//
// All Tauri commands should be defined here
// Commands are the interface between the frontend and the Rust backend
//
// Design principles:
// - Commands should use the OWL module API, not direct SQL
// - Keep commands thin - business logic belongs in OWL module
// - Each command should have tests using tauri::test::mock_app()

mod setup;
mod entity;

pub use setup::*;
pub use entity::*;
