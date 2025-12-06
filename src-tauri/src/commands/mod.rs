// Command modules
mod general__greet;
mod db__get_stats;
mod triple__get_all;
mod node__get_triples;
mod node__get_backlinks;
mod node__get_statistics;
mod node__get_icon;
mod property__get_applicable;
mod graph__get_ontology;
mod class__search;
mod setup__check_initial;
mod system__get_info;
mod setup__complete_initial;

// Re-export command functions
pub use general__greet::greet;
pub use db__get_stats::get_db_stats;
pub use triple__get_all::get_all_triples;
pub use node__get_triples::get_node_triples;
pub use node__get_backlinks::get_node_backlinks;
pub use node__get_statistics::get_node_statistics;
pub use node__get_icon::get_node_icon;
pub use property__get_applicable::get_applicable_properties;
pub use graph__get_ontology::get_ontology_graph;
pub use class__search::search_classes;
pub use setup__check_initial::check_initial_setup;
pub use system__get_info::get_system_info;
pub use setup__complete_initial::complete_initial_setup;
