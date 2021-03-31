use std::any::Any;
use std::ffi::OsStr;

use dynamic_reload::{DynamicReload, Lib, Search, Symbol};

use crate::errors::*;

pub trait Plugin: Any + Send + Sync {
    fn name(&self) -> &'static str;
    fn on_load(&self) {}
    fn on_unload(&self) {}
}

#[macro_export]
macro_rules! declare_plugin {
    ($plugin:expr) => {
        #[no_mangle]
        pub fn _plugin_create() -> Box<dyn Plugin> {
            Box::new($plugin)
        }
    };
}

pub struct PluginManager {
    reload_handler: DynamicReload,
    plugins: Vec<Box<dyn Plugin>>,
    loaded_libraries: Vec<Lib>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self {
            reload_handler: DynamicReload::new(None, Some("target/"), Search::Default),
            plugins: vec![],
            loaded_libraries: vec![],
        }
    }
}

impl PluginManager {
    pub fn load_plugin(&mut self, filename: impl AsRef<OsStr>) -> Result<()> {
        type PluginCreate = fn() -> Box<dyn Plugin>;
        let lib = unsafe { Library::new(filename).chain_err(|| "Failed to load library")? };
        self.loaded_libraries.push(lib);
        let lib = self.loaded_libraries.last().unwrap();
        let plugin_create: Symbol<PluginCreate> = unsafe {
            lib.get(b"_plugin_create")
                .chain_err(|| "Failed to find _plugin_create symbol")?
        };
        let plugin = plugin_create();
        self.plugins.push(plugin);
        let plugin = self.plugins.last().unwrap();
        println!("Loaded plugin: {}", plugin.name());
        plugin.on_load();
        Ok(())
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        println!("Unloading plugins");
        for plugin in self.plugins.drain(..) {
            plugin.on_unload();
        }
        for library in self.loaded_libraries.drain(..) {
            drop(library);
        }
    }
}
