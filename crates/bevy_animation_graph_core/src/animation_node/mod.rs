pub mod dyn_node_like;
pub mod serial;

use std::fmt::Debug;

use bevy::{
    platform::collections::HashMap,
    prelude::{Deref, DerefMut},
    reflect::prelude::*,
};
use uuid::Uuid;

use crate::{
    animation_graph::{NodeId, PinId, TimeUpdate},
    animation_node::dyn_node_like::DynNodeLike,
    context::{
        new_context::NodeContext,
        spec_context::{NodeSpec, SpecContext, SpecResources},
    },
    errors::GraphError,
};

#[reflect_trait]
pub trait NodeLike: NodeLikeClone + Send + Sync + Debug + Reflect + 'static {
    #[allow(unused_variables)]
    fn duration(&self, ctx: NodeContext) -> Result<(), GraphError> {
        Ok(())
    }

    fn update(&self, ctx: NodeContext) -> Result<(), GraphError>;
    fn spec(&self, ctx: SpecContext) -> Result<(), GraphError>;

    /// "Last-resort" method to fetch a time update from a node that hasn't exposed it yet,
    /// but may already have it ready internally. For example, FSM nodes.
    fn try_get_time(&self, ctx: NodeContext, pin: PinId) -> Result<TimeUpdate, GraphError> {
        Err(GraphError::ExtraTimeUpdateNotAvailable {
            graph: ctx.graph_context.context_id,
            node: ctx.node_id,
            pin,
        })
    }

    /// The name of this node.
    fn display_name(&self) -> String;
}

pub trait NodeLikeClone {
    fn clone_node_like(&self) -> Box<dyn NodeLike>;
}

impl<T> NodeLikeClone for T
where
    T: 'static + NodeLike + Clone,
{
    fn clone_node_like(&self) -> Box<dyn NodeLike> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn NodeLike> {
    fn clone(&self) -> Self {
        self.clone_node_like()
    }
}

#[derive(Clone, Reflect, Debug, Default)]
pub struct PinOrdering {
    keys: HashMap<PinId, i32>,
}

impl PinOrdering {
    pub fn new(keys: impl Into<HashMap<PinId, i32>>) -> Self {
        Self { keys: keys.into() }
    }

    pub fn pin_key(&self, pin_id: &PinId) -> i32 {
        self.keys.get(pin_id).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Deref, DerefMut, Reflect)]
pub struct AnimationNode {
    pub id: NodeId,
    pub name: String,
    #[deref]
    pub inner: DynNodeLike,
    pub should_debug: bool,
}

impl AnimationNode {
    #[must_use]
    pub fn new(name: impl Into<String>, inner: impl NodeLike) -> Self {
        Self {
            name: name.into(),
            inner: DynNodeLike::new(inner),
            should_debug: false,
            id: NodeId(Uuid::new_v4()),
        }
    }

    pub fn new_spec(&self, resources: SpecResources) -> Result<NodeSpec, GraphError> {
        let mut spec = NodeSpec::default();
        let ctx = SpecContext::new(resources, &mut spec);
        self.spec(ctx)?;
        Ok(spec)
    }

    pub fn inner_ref(&self) -> &dyn NodeLike {
        self.inner.0.as_ref()
    }

    pub fn try_inner_downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.inner_ref().as_any().downcast_ref()
    }
}
