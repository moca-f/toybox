use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::lazy::SyncLazy;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use tb_core::algorithm::topological_sort::{TopologicalGraph, VisitorWithFlag};

use crate::scheduler::Runnable;
use crate::world::ResourceId;
use crate::*;

pub struct SystemRegistry {
    systems: HashMap<TypeId, &'static SystemInfo>,
    resources_info: HashMap<ResourceId, ResourceInfo>,
    system_topological_graph:
        tb_core::algorithm::topological_sort::TopologicalGraph<&'static SystemInfo>,
    systems_changed: bool,
}

impl SystemRegistry {
    pub fn add_system_infos(infos: Box<dyn Iterator<Item = &'static SystemInfo>>) {
        let mut sr = Self::write();
        sr.systems_changed = true;
        let systems = &mut sr.systems;
        for info in infos {
            systems.insert(info.system_type_id(), info);
        }
    }

    pub fn systems() -> (
        VisitorWithFlag<'static, &'static SystemInfo, usize>,
        RwLockReadGuard<'static, SystemRegistry>,
    ) {
        let sr = SystemRegistry::read();
        let sr = if sr.systems_changed {
            drop(sr);
            let mut sr = Self::write();
            sr.refresh();
            drop(sr);
            Self::read()
        } else {
            sr
        };
        let graph: &TopologicalGraph<&'static SystemInfo> =
            unsafe { std::mem::transmute(&sr.system_topological_graph) };
        (graph.visit_with_flag(), sr)
    }

    fn get_instance() -> &'static RwLock<SystemRegistry> {
        static SYSTEM_REGISTRY: SyncLazy<RwLock<SystemRegistry>> = SyncLazy::new(|| {
            let mut registry = SystemRegistry {
                systems: Default::default(),
                resources_info: Default::default(),
                system_topological_graph: Default::default(),
                systems_changed: true,
            };

            for system_info in inventory::iter::<SystemInfo> {
                registry
                    .systems
                    .insert(system_info.system_type_id(), system_info);
            }

            RwLock::new(registry)
        });

        &SYSTEM_REGISTRY
    }

    fn write() -> RwLockWriteGuard<'static, SystemRegistry> {
        Self::get_instance().write().unwrap()
    }
    fn read() -> RwLockReadGuard<'static, SystemRegistry> {
        Self::get_instance().read().unwrap()
    }

    fn refresh(&mut self) {
        if !self.systems_changed {
            return;
        }

        self.systems_changed = false;

        let resources_info = &mut self.resources_info;
        resources_info.clear();
        self.systems.values().for_each(|system_info| {
            system_info
                .reads_before_write
                .iter()
                .for_each(|resource_id| {
                    resources_info
                        .entry(*resource_id)
                        .or_insert_with(ResourceInfo::default)
                        .read_before_write_systems
                        .insert(system_info);
                });
            system_info.writes.iter().for_each(|resource_id| {
                resources_info
                    .entry(*resource_id)
                    .or_insert_with(ResourceInfo::default)
                    .write_systems
                    .insert(system_info);
            });
            system_info
                .reads_after_write
                .iter()
                .for_each(|resource_id| {
                    resources_info
                        .entry(*resource_id)
                        .or_insert_with(ResourceInfo::default)
                        .read_after_write_systems
                        .insert(system_info);
                });
        });

        let graph = &mut self.system_topological_graph;
        graph.clear();
        self.systems.values().for_each(|system_info| {
            graph.add_item(system_info);
            system_info.writes.iter().for_each(|write_resource| {
                let write_resource_info = resources_info.get(write_resource).unwrap();
                write_resource_info
                    .read_before_write_systems
                    .iter()
                    .for_each(|read_before_write_system| {
                        graph.add_dependency(system_info, read_before_write_system);
                    });
                write_resource_info
                    .read_after_write_systems
                    .iter()
                    .for_each(|read_after_write_system| {
                        graph.add_dependency(read_after_write_system, system_info);
                    });
            });
        });

        self.systems.values().for_each(|system_info| {
            system_info.writes.iter().for_each(|write_resource| {
                let write_resource_info = resources_info.get(write_resource).unwrap();
                write_resource_info
                    .write_systems
                    .iter()
                    .for_each(|write_system| {
                        graph.add_dependency_if_non_inverse(write_system, system_info);
                    })
            });
        });
    }
}

#[derive(Default)]
pub struct ResourceInfo {
    read_before_write_systems: HashSet<&'static SystemInfo>,
    write_systems: HashSet<&'static SystemInfo>,
    read_after_write_systems: HashSet<&'static SystemInfo>,
}

pub struct SystemInfo {
    type_id: TypeId,
    name: &'static str,
    reads_before_write: Vec<ResourceId>,
    reads_after_write: Vec<ResourceId>,
    writes: Vec<ResourceId>,
    create: fn() -> Box<dyn Runnable>,
}

impl SystemInfo {
    pub fn new<S>() -> Self
    where
        for<'r> S: 'static + std::default::Default + System<'r>,
    {
        let type_id = std::any::TypeId::of::<S>();
        let name = std::any::type_name::<S>();
        println!(
            "new system info. system type id: {:?}, name: {}",
            type_id, name
        );

        Self {
            type_id,
            name,
            reads_before_write: S::SystemData::reads_before_write(),
            reads_after_write: S::SystemData::reads_after_write(),
            writes: S::SystemData::writes(),
            create: || Box::new(S::default()),
        }
    }

    pub fn name(&self) -> &str {
        self.name
    }

    pub fn system_type_id(&self) -> TypeId {
        self.type_id
    }
}

impl PartialEq for &SystemInfo {
    fn eq(&self, other: &Self) -> bool {
        (*self as *const SystemInfo).eq(&(*other as *const SystemInfo))
    }
}

impl Eq for &SystemInfo {}

impl Hash for &SystemInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (*self as *const SystemInfo).hash(state);
    }
}

inventory::collect!(SystemInfo);

#[cfg(test)]
mod tests {
    use crate::*;

    #[system]
    struct TestSystem {
        _value: i32,
    }

    impl System<'_> for TestSystem {
        type SystemData = ();

        fn run(&mut self, _system_data: Self::SystemData) {}
    }

    #[test]
    fn it_works() {
        let mut has = false;
        for _x in SystemRegistry::systems().0 {
            has = true;
        }
        assert!(has);
        let mut has = false;
        for _x in SystemRegistry::systems().0 {
            has = true;
        }
        assert!(has);
    }
}
