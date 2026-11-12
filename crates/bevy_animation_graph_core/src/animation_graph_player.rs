use std::sync::Arc;

use bevy::{
    asset::prelude::*,
    color::{Color, palettes::css::WHITE},
    ecs::prelude::*,
    gizmos::gizmos::Gizmos,
    platform::collections::HashMap,
    reflect::prelude::*,
    transform::components::Transform,
};

use crate::{
    animation_graph::{AnimationGraph, DEFAULT_OUTPUT_POSE, GraphInputPin, PinId, TimeUpdate},
    context::{
        deferred_gizmos::{
            CustomRelativeDrawCommand, CustomRelativeDrawCommandReference, DeferredGizmos,
            DeferredGizmosContext,
        },
        graph_context_arena::GraphContextArena,
        io_env::IoOverrides,
        pose_fallback::PoseFallbackContext,
        system_resources::SystemResources,
    },
    edge_data::{
        DataValue,
        events::{AnimationEvent, EventQueue, SampledEvent},
    },
    errors::GraphError,
    pose::{BoneId, Pose, RootMotionDelta},
    ragdoll::{bone_mapping::RagdollBoneMap, definition::Ragdoll, spawning::SpawnedRagdoll},
    skeleton::Skeleton,
    space_conversion::SpaceConversionContext,
};

/// Evaluates a player's graph less often than every frame, e.g. for distant
/// characters. Add it to the entity with the [`AnimationGraphPlayer`].
///
/// Frame deltas keep accumulating between updates, so clips and transitions
/// keep their timing; they just advance in larger steps. Skipped frames also
/// skip the entity map rebuild and the bone writes.
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct AnimationUpdateRate {
    /// Seconds between graph updates; zero or less updates every frame.
    pub interval: f32,
    /// Offset in [0, 1) of the update slot within the interval, so players
    /// sharing an interval spread their updates across frames.
    pub phase: f32,
    /// Blend the bones from the previous to the latest evaluated pose on the
    /// frames in between. Smooth, but shows the pose one interval late and
    /// writes the bones every frame.
    pub interpolate: bool,
}

impl AnimationUpdateRate {
    /// Every frame, as without the component.
    pub const EVERY_FRAME: Self = Self {
        interval: 0.0,
        phase: 0.0,
        interpolate: false,
    };

    /// Advances `clock` by `dt` and reports whether an update slot was crossed.
    pub fn advance(&self, clock: &mut f64, dt: f32) -> bool {
        let before = *clock;
        *clock += dt as f64;
        if !(self.interval > 0.0) {
            return true;
        }
        let interval = self.interval as f64;
        let offset = self.phase.rem_euclid(1.0) as f64 * interval;
        ((*clock + offset) / interval).floor() > ((before + offset) / interval).floor()
    }
}

#[derive(Default, Reflect, Clone, Copy)]
pub enum PlaybackState {
    Paused,
    #[default]
    Play,
    PlayOneFrame,
}

impl PlaybackState {
    pub fn is_paused(&self) -> bool {
        matches!(self, PlaybackState::Paused)
    }
}

#[derive(Reflect, Clone, Default)]
pub enum AnimationSource {
    Graph(Handle<AnimationGraph>),
    Pose(Pose),
    #[default]
    None,
}

impl AnimationSource {
    pub fn is_none(&self) -> bool {
        matches!(self, AnimationSource::None)
    }

    pub fn is_graph(&self) -> bool {
        matches!(self, AnimationSource::Graph(_))
    }

    pub fn is_pose(&self) -> bool {
        matches!(self, AnimationSource::Pose(_))
    }
}

/// Animation controls
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct AnimationGraphPlayer {
    pub(crate) playback_state: PlaybackState,
    pub(crate) animation: AnimationSource,
    pub(crate) skeleton: Handle<Skeleton>,
    pub(crate) ragdoll: Option<Handle<Ragdoll>>,
    pub(crate) ragdoll_bone_map: Option<Handle<RagdollBoneMap>>,
    pub(crate) spawned_ragdoll: Option<SpawnedRagdoll>,
    pub(crate) context_arena: Option<GraphContextArena>,
    pub(crate) elapsed: f32,
    pub(crate) pending_update: TimeUpdate,
    pub(crate) deferred_gizmos: DeferredGizmos,
    pub(crate) debug_draw_bones: Vec<(BoneId, Color, bool)>,
    #[reflect(ignore)]
    pub(crate) debug_draw_custom: Vec<CustomRelativeDrawCommand>,
    pub(crate) entity_map: HashMap<BoneId, Entity>,
    pub(crate) queued_events: EventQueue,
    pub(crate) outputs: HashMap<PinId, DataValue>,
    /// The entity map must be rebuilt before the next graph update.
    pub(crate) entity_map_dirty: bool,
    /// Accumulated time for [`AnimationUpdateRate`] slots.
    pub(crate) update_clock: f64,
    /// Seconds since the graph was last evaluated.
    pub(crate) since_update: f32,
    /// [`AnimationUpdateRate`] skipped this frame's graph update.
    pub(crate) skipped_this_frame: bool,
    /// Output pose before the latest update, for interpolated update rates.
    #[reflect(ignore)]
    pub(crate) previous_output_pose: Option<Pose>,
    /// Blend factor from `previous_output_pose` to the current pose this frame.
    pub(crate) interpolation_alpha: Option<f32>,

    io_overrides: IoOverrides,

    global_input_data: HashMap<PinId, DataValue>,

    /// Error that ocurred during graph evaluation in the last frame
    #[reflect(ignore)]
    error: Option<GraphError>,
}

impl AnimationGraphPlayer {
    pub const USER_EVENTS: &'static str = "user_events";

    /// Create a new animation graph player, with no graph playing
    pub fn new(skeleton: Handle<Skeleton>) -> Self {
        Self {
            skeleton,
            ..Default::default()
        }
    }

    pub fn get_context_arena(&self) -> Option<&GraphContextArena> {
        self.context_arena.as_ref()
    }

    pub fn set_animation(&mut self, animation: AnimationSource) {
        self.animation = animation;
    }

    pub fn skeleton(&self) -> &Handle<Skeleton> {
        &self.skeleton
    }

    /// Set the animation graph to play
    pub fn with_graph(mut self, animation: Handle<AnimationGraph>) -> Self {
        self.context_arena = Some(GraphContextArena::new(animation.id()));
        self.animation = AnimationSource::Graph(animation);
        self
    }

    /// Clear all input parameters for the animation graph
    pub fn clear_input_parameters(&mut self) {
        self.io_overrides.clear();
    }

    /// Configure an input parameter for the animation graph
    pub fn set_input_data(&mut self, input_pin: impl Into<PinId>, value: DataValue) {
        self.io_overrides
            .data
            .insert(GraphInputPin::Passthrough(input_pin.into()), value);
    }

    /// Return an input parameter for the animation graph
    pub fn get_input_data(&self, input_pin: PinId) -> Option<DataValue> {
        self.io_overrides
            .data
            .get(&GraphInputPin::Passthrough(input_pin))
            .cloned()
    }

    /// Configure an input parameter for the animation graph
    pub fn set_global_input_data(
        &mut self,
        input_pin: impl Into<PinId>,
        value: impl Into<DataValue>,
    ) {
        self.global_input_data
            .insert(input_pin.into(), value.into());
    }

    /// Return an input parameter for the animation graph
    pub fn get_global_input_data(&self, input_pin: impl Into<PinId>) -> Option<&DataValue> {
        let input_pin: PinId = input_pin.into();
        self.global_input_data.get(&input_pin)
    }

    /// Start playing an animation, resetting state of the player.
    /// This will use a linear blending between the previous and the new animation to make a smooth transition.
    pub fn start(&mut self, handle: Handle<AnimationGraph>) -> &mut Self {
        self.context_arena = Some(GraphContextArena::new(handle.id()));
        self.animation = AnimationSource::Graph(handle);
        self.elapsed = 0.;
        self.playback_state = PlaybackState::Play;
        self
    }

    /// Queue an event to make available to the animation graph this frame. This event queue is
    /// cleared every frame.
    pub fn send_event(&mut self, event: AnimationEvent) {
        self.queued_events.events.push(SampledEvent {
            event,
            weight: 1.,
            percentage: 1.,
            track: None,
        });
    }

    pub fn queue_time_update(&mut self, update: TimeUpdate) {
        self.pending_update = self.pending_update.combine(&update);
    }

    /// Query the animation graph with the latest time update and inputs
    pub(crate) fn update(&mut self, system_resources: &SystemResources, root_entity: Entity) {
        self.outputs.clear();
        self.io_overrides.data.insert(
            GraphInputPin::Passthrough(Self::USER_EVENTS.into()),
            std::mem::take(&mut self.queued_events).into(),
        );

        let AnimationSource::Graph(graph_handle) = &self.animation else {
            return;
        };

        let Some(graph) = system_resources.animation_graph_assets.get(graph_handle) else {
            return;
        };

        match graph.query_with_env(
            self.pending_update.clone(),
            self.context_arena.as_mut().unwrap(),
            system_resources,
            &self.io_overrides,
            root_entity,
            &self.entity_map,
            &mut self.deferred_gizmos,
            &self.global_input_data,
        ) {
            Ok(outputs) => {
                self.error = None;
                self.outputs = outputs;
            }
            Err(error) => {
                self.error = Some(error);
            }
        };

        if let Some(pose) = self.outputs.get(DEFAULT_OUTPUT_POSE) {
            let _ = pose.as_pose().map(|p| self.elapsed = p.timestamp);
        }

        self.pending_update = TimeUpdate::Delta(0.);
    }

    pub fn gizmo_for_bones(&mut self, bones: impl IntoIterator<Item = BoneId>) {
        self.debug_draw_bones
            .extend(bones.into_iter().map(|b| (b, WHITE.into(), false)));
    }

    pub fn gizmo_for_bones_with_color(
        &mut self,
        bones: impl IntoIterator<Item = (BoneId, Color, bool)>,
    ) {
        self.debug_draw_bones.extend(bones);
    }

    pub fn custom_relative_gizmo(&mut self, gizmo: CustomRelativeDrawCommand) {
        self.debug_draw_custom.push(gizmo);
    }

    pub fn gizmo_relative_to_root(
        &mut self,
        f: impl Fn(Transform, &mut Gizmos) + Send + Sync + 'static,
    ) {
        self.debug_draw_custom.push(CustomRelativeDrawCommand {
            reference: CustomRelativeDrawCommandReference::Root,
            f: Arc::new(f),
        });
    }

    pub(crate) fn debug_draw_bones(&mut self, system_resources: &SystemResources) {
        if self.debug_draw_bones.is_empty() && self.debug_draw_custom.is_empty() {
            return;
        }

        let mut bones = std::mem::take(&mut self.debug_draw_bones);
        let mut custom_gizmos = std::mem::take(&mut self.debug_draw_custom);

        let skeleton_handle = self.skeleton.clone();

        let mut gizmos = DeferredGizmosContext {
            gizmos: &mut self.deferred_gizmos,
            resources: system_resources,
            entity_map: &self.entity_map,
            space_conversion: SpaceConversionContext {
                pose_fallback: PoseFallbackContext {
                    entity_map: &self.entity_map,
                    resources: system_resources,
                    fallback_to_identity: false,
                },
            },
        };

        let Some(skeleton) = system_resources.skeleton_assets.get(&skeleton_handle) else {
            return;
        };

        for (bone_id, color, draw_children) in bones.drain(..) {
            gizmos.bone_gizmo(bone_id, color.into(), draw_children, skeleton, None);
        }

        for custom_cmd in custom_gizmos.drain(..) {
            gizmos.relative_custom_gizmo(custom_cmd, skeleton, None);
        }
    }

    pub fn pause(&mut self) {
        self.playback_state = PlaybackState::Paused;
    }

    pub fn resume(&mut self) {
        self.playback_state = PlaybackState::Play;
    }

    pub fn is_paused(&self) -> bool {
        self.playback_state.is_paused()
    }

    pub fn playback_state(&self) -> PlaybackState {
        self.playback_state
    }

    pub fn play_one_frame(&mut self) {
        self.playback_state = PlaybackState::PlayOneFrame;
    }

    pub fn reset(&mut self) {
        self.pending_update = TimeUpdate::Absolute(0.);
    }

    pub fn seek(&mut self, time: f32) {
        self.pending_update = TimeUpdate::Absolute(time);
    }

    pub fn get_animation_source(&self) -> &AnimationSource {
        &self.animation
    }

    /// If graph evaluation produced an error in the last frame return the error, otherwise return
    /// `None`.
    pub fn get_error(&self) -> Option<GraphError> {
        self.error.clone()
    }

    pub fn get_outputs(&self) -> &HashMap<PinId, DataValue> {
        &self.outputs
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// Gets the default output pose from the graph, or the forced pose if set.
    pub fn get_default_output_pose(&self) -> Option<&Pose> {
        match &self.animation {
            AnimationSource::Graph(_) => self.outputs.get(DEFAULT_OUTPUT_POSE)?.as_pose().ok(),
            AnimationSource::Pose(pose) => Some(pose),
            AnimationSource::None => None,
        }
    }

    /// Gets the root motion delta from the default output pose, if present.
    /// `None` on frames whose graph update was skipped, so the delta of one
    /// update is not applied again on the frames until the next.
    pub fn get_root_motion(&self) -> Option<&RootMotionDelta> {
        if self.skipped_this_frame {
            return None;
        }
        self.get_default_output_pose()?.root_motion.as_ref()
    }

    /// Whether [`AnimationUpdateRate`] skipped this frame's graph update.
    pub fn skipped_this_frame(&self) -> bool {
        self.skipped_this_frame
    }

    pub fn set_default_output_pose(&mut self, pose: Pose) {
        self.outputs.insert(DEFAULT_OUTPUT_POSE.into(), pose.into());
    }

    pub fn bone_entity(&self, bone_id: BoneId) -> Option<Entity> {
        self.entity_map.get(&bone_id).copied()
    }

    pub fn get_spawned_ragdoll(&self) -> Option<&SpawnedRagdoll> {
        self.spawned_ragdoll.as_ref()
    }
}

#[cfg(test)]
mod update_rate_tests {
    use super::AnimationUpdateRate;

    /// Frames (of `frames` at 60 Hz) on which `rate` updates.
    fn update_frames(rate: AnimationUpdateRate, frames: usize) -> Vec<usize> {
        let mut clock = 0.0;
        (0..frames)
            .filter(|_| rate.advance(&mut clock, 1.0 / 60.0))
            .collect()
    }

    #[test]
    fn update_slots_follow_interval_and_phase() {
        assert_eq!(update_frames(AnimationUpdateRate::EVERY_FRAME, 60).len(), 60);
        let rate = |phase| AnimationUpdateRate { interval: 0.1, phase, interpolate: false };
        assert_eq!(update_frames(rate(0.0), 60).len(), 10);
        // Same rate, different phase: different frames.
        assert_ne!(update_frames(rate(0.0), 12), update_frames(rate(0.5), 12));
    }
}
