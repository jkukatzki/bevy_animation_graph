use bevy::prelude::*;
use bevy_animation_graph_core::{
    animation_node::{NodeLike, ReflectNodeLike},
    context::{new_context::NodeContext, spec_context::SpecContext},
    edge_data::DataSpec,
    errors::GraphError,
    symmetry::{config::SymmetryConfig, flip_pose, serial::SymmetryConfigSerial},
};
use serde::{Deserialize, Serialize};

#[derive(Reflect, Clone, Debug)]
#[reflect(Default, NodeLike, Serialize, Deserialize)]
#[type_path = "bevy_animation_graph::builtin_nodes"]
pub struct FlipLRNode {
    pub config: SymmetryConfig,
}

impl Default for FlipLRNode {
    fn default() -> Self {
        Self::new(SymmetryConfig::default())
    }
}

impl FlipLRNode {
    pub const IN_POSE: &'static str = "pose";
    pub const IN_TIME: &'static str = "time";
    pub const OUT_POSE: &'static str = "pose";

    pub fn new(config: SymmetryConfig) -> Self {
        Self { config }
    }
}

impl NodeLike for FlipLRNode {
    fn duration(&self, mut ctx: NodeContext) -> Result<(), GraphError> {
        let duration = ctx.duration_back(Self::IN_TIME)?;
        ctx.set_duration_fwd(duration);
        Ok(())
    }

    fn update(&self, mut ctx: NodeContext) -> Result<(), GraphError> {
        let input = ctx.time_update_fwd()?;
        ctx.set_time_update_back(Self::IN_TIME, input);
        let in_pose = ctx.data_back(Self::IN_POSE)?.into_pose()?;
        ctx.set_time(in_pose.timestamp);
        let Some(skeleton) = ctx
            .graph_context
            .resources
            .skeleton_assets
            .get(&in_pose.skeleton)
        else {
            return Err(GraphError::SkeletonMissing(ctx.node_id));
        };
        let flipped_pose = flip_pose(&in_pose, &self.config, skeleton)?;
        ctx.set_data_fwd(Self::OUT_POSE, flipped_pose);

        Ok(())
    }

    fn spec(&self, mut ctx: SpecContext) -> Result<(), GraphError> {
        ctx //
            .add_input_data(Self::IN_POSE, DataSpec::Pose)
            .add_input_time(Self::IN_TIME);
        ctx //
            .add_output_data(Self::OUT_POSE, DataSpec::Pose)
            .add_output_time();

        Ok(())
    }

    fn display_name(&self) -> String {
        "🚻 Flip Left/Right".into()
    }
}

#[derive(Clone, Reflect, Serialize, Deserialize)]
pub struct FlipLRProxy {
    pub config: SymmetryConfigSerial,
}

impl From<&FlipLRNode> for FlipLRProxy {
    fn from(value: &FlipLRNode) -> Self {
        FlipLRProxy {
            config: SymmetryConfigSerial::from_value(&value.config),
        }
    }
}

impl TryFrom<&FlipLRProxy> for FlipLRNode {
    type Error = regex::Error;

    fn try_from(value: &FlipLRProxy) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            config: value.config.to_value()?,
        })
    }
}

impl Serialize for FlipLRNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        FlipLRProxy::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FlipLRNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ser = FlipLRProxy::deserialize(deserializer)?;

        Self::try_from(&ser)
            .map_err(|_| serde::de::Error::custom("Failed to parse regular expression"))
    }
}
