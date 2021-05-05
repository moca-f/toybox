use bimap::BiHashMap;

use tb_ecs::*;

pub struct Prefab {
    root_entity: Entity,
    components: Vec<Box<dyn ComponentsInPrefab>>,
}

trait ComponentsInPrefab: Sync {
    fn attach(&self, world: &mut World, link: &mut PrefabLink);
}

pub trait ConvertToWorld {
    fn convert_to_world(&mut self, link: &mut PrefabLink, entities: &mut Entities);
}

#[component]
#[derive(Default)]
pub struct PrefabLink {
    local_entity_to_world_map: BiHashMap<Entity, Entity>,
}

struct ComponentStorageInPrefab<C: Component> {
    components: Vec<C>,
    entities: Vec<Entity>,
}

impl<C: Component> ComponentStorageInPrefab<C> {
    pub(crate) fn insert(&mut self, entity: Entity, component: C) {
        self.entities.push(entity);
        self.components.push(component);
    }
}

impl<C: Component> Default for ComponentStorageInPrefab<C> {
    fn default() -> Self {
        Self {
            components: vec![],
            entities: vec![],
        }
    }
}

impl<C> ComponentsInPrefab for ComponentStorageInPrefab<C>
where
    C: Component,
{
    default fn attach(&self, world: &mut World, link: &mut PrefabLink) {
        world.insert_components::<C>();
        world.insert(Entities::default);
        let mut components_in_world = WriteComponents::<C>::fetch(world);
        let entities = world.fetch::<Entities>();
        let (entity, components) = (self.entities.iter(), self.components.iter());
        entity.zip(components).for_each(|(&entity, component)| {
            components_in_world.insert(link.build_link(entity, entities), component.clone());
        });
    }
}

impl<C> ComponentsInPrefab for ComponentStorageInPrefab<C>
where
    for<'e> C: ComponentWithEntityRef<'e>,
{
    fn attach(&self, world: &mut World, link: &mut PrefabLink) {
        world.insert_components::<C>();
        let mut components_in_world = WriteComponents::<C>::fetch(world);
        let entities = world.fetch_mut::<Entities>();
        let (entity, components) = (self.entities.iter(), self.components.iter());
        entity.zip(components).for_each(|(&entity, component)| {
            let mut component: C = component.clone();
            let mut entity_ref = component.get_entity_ref();
            ConvertToWorld::convert_to_world(&mut entity_ref, link, entities);
            drop(entity_ref);
            components_in_world.insert(link.build_link(entity, entities), component);
        });
    }
}

impl PrefabLink {
    fn build_link(&mut self, local: Entity, entities: &Entities) -> Entity {
        match self.local_entity_to_world_map.get_by_left(&local) {
            None => {
                let entity = entities.new_entity();
                match self
                    .local_entity_to_world_map
                    .insert_no_overwrite(local, entity)
                {
                    Ok(_) => {}
                    Err(_) => unreachable!(),
                }
                entity
            }
            Some(entity) => *entity,
        }
    }
}

impl Prefab {
    pub(crate) fn attach(&self, world: &mut World) {
        let mut link = PrefabLink::default();
        for components in &self.components {
            components.attach(world, &mut link);
        }
        world.insert(Entities::default);
        world.insert_components::<PrefabLink>();
        let mut prefab_links = WriteComponents::<PrefabLink>::fetch(world);
        prefab_links.insert(link.build_link(self.root_entity, world.fetch_mut()), link);
    }
}

impl<E: EntityRef> ConvertToWorld for E {
    default fn convert_to_world(&mut self, link: &mut PrefabLink, entities: &mut Entities) {
        self.for_each(&mut |e: &mut Entity| {
            *e = link.build_link(*e, entities);
        });
    }
}

#[cfg(test)]
mod tests {
    use tb_ecs::*;

    use crate::prefab::{ComponentStorageInPrefab, ComponentWithEntityRef, Prefab};

    #[component]
    struct Component0 {
        value: i32,
    }

    #[component]
    struct Component1 {
        entity_a: Entity,
    }

    #[component]
    struct Component2 {
        entity_a: Entity,
        entity_b: Entity,
    }

    #[test]
    fn convert_to_world() {
        let entities: Vec<Entity> = {
            let mut world = World::default();
            let entities = world.insert(Entities::default);
            (0..16).map(|_i| entities.new_entity()).collect()
        };

        let prefab = {
            let mut components0 = ComponentStorageInPrefab::<Component0>::default();
            components0.insert(Entity::new(10), Component0 { value: 10 });
            let mut components1 = ComponentStorageInPrefab::<Component1>::default();
            components1.insert(
                Entity::new(7),
                Component1 {
                    entity_a: entities[15],
                },
            );
            let mut components2 = ComponentStorageInPrefab::<Component2>::default();
            components2.insert(
                Entity::new(15),
                Component2 {
                    entity_a: entities[7],
                    entity_b: entities[10],
                },
            );
            let components: Vec<Box<dyn crate::prefab::ComponentsInPrefab>> = vec![
                Box::new(components0),
                Box::new(components1),
                Box::new(components2),
            ];
            Prefab {
                root_entity: Entity::new(15),
                components,
            }
        };

        let mut world = World::default();
        prefab.attach(&mut world);

        let (components0, components1, components2) = <(
            RAWComponents<Component0>,
            RAWComponents<Component1>,
            RAWComponents<Component2>,
        ) as SystemData>::fetch(&world);
        for component0 in components0.join() {
            let component0: &Component0 = component0;
            assert_eq!(component0.value, 10);
        }
        for component1 in components1.join() {
            let component1: &Component1 = component1;
            assert_eq!(component1.entity_a, Entity::new(1));
        }
        for component2 in components2.join() {
            let component2: &Component2 = component2;
            assert_eq!(component2.entity_a, Entity::new(2));
            assert_eq!(component2.entity_b, Entity::new(0));
        }
    }
}
