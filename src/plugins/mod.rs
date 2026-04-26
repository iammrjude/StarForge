pub mod interface;
pub mod loader;
pub mod registry;

pub use interface::{Plugin, PluginDeclaration};
pub use loader::PluginManager;
