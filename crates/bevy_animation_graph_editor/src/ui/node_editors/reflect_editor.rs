use bevy::ecs::world::World;
use bevy_animation_graph::core::animation_node::NodeLike;

use crate::ui::{node_editors::DynNodeEditor, utils::using_inspector_env};

#[derive(Default)]
pub struct ReflectNodeEditor;

impl DynNodeEditor for ReflectNodeEditor {
    fn show_dyn(
        &self,
        ui: &mut egui::Ui,
        world: &mut World,
        node: &mut dyn NodeLike,
    ) -> egui::Response {
        let mut response = ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover());
        let changed = using_inspector_env(world, |mut env| {
            env.ui_for_reflect(node.as_partial_reflect_mut(), ui)
        });

        if changed {
            response.mark_changed();
        }

        response
    }
}
