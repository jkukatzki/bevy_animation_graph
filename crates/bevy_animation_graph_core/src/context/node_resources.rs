use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use bevy::{ecs::prelude::*, platform::collections::HashMap};

/// A type-erased resource map that animation graph nodes can read during
/// evaluation. Because graph nodes execute inside `SystemResources`—which
/// itself is a `SystemParam`—adding arbitrary `Res<T>` fields there is not
/// possible for downstream crates. Instead, apps insert their own data into
/// this map once per frame (or whenever the data changes) and nodes pull it
/// out by type via [`GraphNodeResources::get`].
///
/// # Usage (app side)
/// ```rust,ignore
/// fn sync_my_data(
///     my_res: Res<MyResource>,
///     mut gnr: ResMut<GraphNodeResources>,
/// ) {
///     if my_res.is_changed() {
///         gnr.insert(my_res.clone_for_graph());
///     }
/// }
/// ```
///
/// # Usage (node side)
/// ```rust,ignore
/// if let Some(reg) = ctx.graph_context.resources.graph_node_resources.get::<MyResource>() {
///     // use reg
/// }
/// ```
#[derive(Resource, Default)]
pub struct GraphNodeResources {
    map: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl GraphNodeResources {
    /// Insert (or replace) a value accessible by its concrete type `T`.
    pub fn insert<T: Any + Send + Sync + 'static>(&mut self, value: T) {
        self.map.insert(TypeId::of::<T>(), Arc::new(value));
    }

    /// Retrieve a shared reference to the value stored under type `T`,
    /// or `None` if nothing has been inserted for that type.
    pub fn get<T: Any + Send + Sync + 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|arc| (**arc).downcast_ref::<T>())
    }

    /// Remove the value stored under type `T`, if any.
    pub fn remove<T: Any + Send + Sync + 'static>(&mut self) {
        self.map.remove(&TypeId::of::<T>());
    }

    /// Returns `true` if a value of type `T` is present.
    pub fn contains<T: Any + Send + Sync + 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }
}
