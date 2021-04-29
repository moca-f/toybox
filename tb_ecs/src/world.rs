use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;

use errors::*;
use tb_core::event_channel::EventChannel;

mod errors {
    pub use tb_core::error::*;

    error_chain! {
        errors {
            Fetch(resource_type_name: String) {
                description("Failed to fetch resource"),
                display("Failed to fetch resource. type_name: {}", resource_type_name),
            }
        }
    }
}

type Resources = HashMap<ResourceId, RefCell<Box<dyn Resource>>>;

#[derive(Default)]
pub struct World {
    resources: Resources,
}

impl World {
    pub fn insert<R: Resource>(&mut self, create: impl FnOnce() -> R) -> &mut R {
        let mut is_new = false;
        let r = self
            .resources
            .entry(ResourceId::new::<R>())
            .or_insert_with(|| {
                is_new = true;
                RefCell::new(Box::new(create()))
            });
        if is_new {
            let components_change_event_channel = self.insert(EventChannel::default);
            components_change_event_channel.push(ResourcesChangeEvent::new());
        }
        self.fetch_mut()
    }

    pub fn try_fetch<R: Resource>(&self) -> errors::Result<&R> {
        self.resources
            .get(&ResourceId::new::<R>())
            .map(|r| unsafe { &*(r.borrow().as_ref() as *const dyn Resource as *const R) })
            .chain_err(|| errors::ErrorKind::Fetch(std::any::type_name::<R>().into()))
    }

    pub fn try_fetch_mut<R: Resource>(&self) -> errors::Result<&mut R> {
        self.resources
            .get(&ResourceId::new::<R>())
            .map(|r| unsafe { &mut *(r.borrow_mut().as_mut() as *mut dyn Resource as *mut R) })
            .chain_err(|| errors::ErrorKind::Fetch(std::any::type_name::<R>().into()))
    }

    pub fn fetch<R: Resource>(&self) -> &R {
        self.try_fetch().unwrap()
    }

    #[allow(clippy::mut_from_ref)]
    pub fn fetch_mut<R: Resource>(&self) -> &mut R {
        self.try_fetch_mut().unwrap()
    }

    pub fn contains<R: Resource>(&self) -> bool {
        self.resources.contains_key(&ResourceId::new::<R>())
    }
}

#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct ResourceId {
    id: TypeId,
}

impl ResourceId {
    pub(crate) fn new<R: Resource + ?Sized>() -> Self {
        ResourceId {
            id: TypeId::of::<R>(),
        }
    }
}

pub trait Resource: 'static {}

impl<R: Any> Resource for R {}

pub struct ResourcesChangeEvent {}

impl ResourcesChangeEvent {
    fn new() -> Self {
        Self {}
    }
}

#[cfg(test)]
mod tests {
    use crate::World;

    struct TestResource {
        value: i32,
    }

    impl TestResource {
        fn new(value: i32) -> Self {
            Self { value }
        }
    }

    #[test]
    fn world_works() {
        let mut world = World::default();
        let test_resource = world.try_fetch::<TestResource>();
        assert!(test_resource.is_err());
        world.insert(|| TestResource::new(10));
        assert!(world.try_fetch::<TestResource>().is_ok());
        assert_eq!(world.fetch::<TestResource>().value, 10);
        let resource: &mut TestResource = world.fetch_mut();
        resource.value = 20;
        assert_eq!(world.fetch::<TestResource>().value, 20);
    }

    #[test]
    #[should_panic(expected = "Error(Fetch(\"tb_ecs::world::tests::TestResource\")")]
    fn fetch_resource_failed() {
        let world = World::default();
        let _test_resource = world.fetch::<TestResource>();
    }

    #[test]
    fn fetch_mut_error() {
        let world = World::default();
        let result = world.try_fetch_mut::<TestResource>();
        assert!(result.is_err());
    }
}
