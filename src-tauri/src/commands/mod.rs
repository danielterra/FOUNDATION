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
mod setup_system_info;
mod entity;
mod shortcuts;
mod logging;
mod ai;
mod chat;
mod chat_attachments;
mod chat_storage;
pub mod widget;

pub use setup::*;
pub use entity::*;
pub use shortcuts::*;
pub use logging::*;
pub use ai::*;
pub use chat::*;
pub use chat_attachments::*;
pub use widget::*;
