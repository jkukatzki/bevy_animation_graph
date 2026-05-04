use bevy::{ecs::world::World, reflect::Reflect};

use crate::ui::{node_editors::NodeEditor, reflect_lib::ReflectWidgetContext};

pub trait ProxyReflectEditor: 'static {
    type Target: 'static;
    type Proxy: Reflect;

    fn value_to_proxy(&self, value: &Self::Target) -> Option<Self::Proxy>;
    fn proxy_to_value(&self, proxy: &Self::Proxy) -> Option<Self::Target>;
}

impl<T: ProxyReflectEditor> NodeEditor for T {
    type Target = <T as ProxyReflectEditor>::Target;

    fn show(
        &self,
        ui: &mut egui::Ui,
        world: &mut World,
        node: &mut Self::Target,
    ) -> egui::Response {
        let Some(mut proxy) = self.value_to_proxy(node) else {
            return ui.allocate_rect(egui::Rect::ZERO, egui::Sense::empty());
        };

        let response = ReflectWidgetContext::scope(world, |ctx| ctx.draw(ui, &mut proxy));

        if response.changed()
            && let Some(value) = self.proxy_to_value(&proxy)
        {
            *node = value;
        }

        response
    }
}
