use bevy_animation_graph::builtin_nodes::flip_lr_node::{FlipLRNode, FlipLRProxy};

use crate::ui::node_editors::{Editable, proxy_reflect_editor::ProxyReflectEditor};

pub struct FlipLrNodeEditor;

impl ProxyReflectEditor for FlipLrNodeEditor {
    type Target = FlipLRNode;
    type Proxy = FlipLRProxy;

    fn value_to_proxy(&self, value: &Self::Target) -> Option<Self::Proxy> {
        Some(value.into())
    }

    fn proxy_to_value(&self, proxy: &Self::Proxy) -> Option<Self::Target> {
        proxy.try_into().ok()
    }
}

impl Editable for FlipLRNode {
    type Editor = FlipLrNodeEditor;

    fn get_editor(&self) -> Self::Editor {
        FlipLrNodeEditor
    }
}
