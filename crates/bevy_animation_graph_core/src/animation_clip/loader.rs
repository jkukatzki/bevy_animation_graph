use bevy::{
    asset::{AssetLoader, AssetPath, Handle, LoadContext, io::Reader},
    gltf::Gltf,
    platform::collections::HashMap,
    reflect::{Reflect, TypePath},
};
use serde::{Deserialize, Serialize};

use super::GraphClip;
use crate::{errors::AssetLoaderError, event_track::EventTrack, utils::normalize_asset_path};

#[derive(Reflect, Serialize, Deserialize, Clone, Debug)]
pub enum GraphClipSource {
    GltfNamed {
        path: AssetPath<'static>,
        animation_name: String,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GraphClipSerial {
    pub source: GraphClipSource,
    pub skeleton: AssetPath<'static>,
    #[serde(default)]
    pub event_tracks: HashMap<String, EventTrack>,
}

/// Pending GltfNamed resolution: the `GraphClip` has been loaded but its curve
/// data has not been extracted yet because the source GLTF is loaded as a
/// deferred dependency (to avoid one HTTP request per clip).
///
/// The `resolve_pending_graph_clips` system in `AnimationGraphCorePlugin`
/// watches for the GLTF to become available and then populates the clip.
#[derive(Clone, Debug)]
pub struct PendingGltfSource {
    pub gltf_handle: Handle<Gltf>,
    pub animation_name: String,
}

#[derive(Default, TypePath)]
pub struct GraphClipLoader;

impl AssetLoader for GraphClipLoader {
    type Asset = GraphClip;
    type Settings = ();
    type Error = AssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes).await?;
        let serial: GraphClipSerial = ron::de::from_bytes(&bytes)?;

        let skeleton = load_context.loader().load(serial.skeleton);

        let clip_mine = match &serial.source {
            GraphClipSource::GltfNamed {
                path,
                animation_name,
            } => {
                // Load the GLTF as a deferred dependency so the asset server
                // deduplicates it across all animation clips referencing the
                // same GLB file.  Curve data is populated later by the
                // `resolve_pending_graph_clips` system once the GLTF is ready.
                let gltf_handle: Handle<Gltf> = load_context.loader().load(path.clone());
                GraphClip {
                    curves: Default::default(),
                    duration: 0.0,
                    skeleton,
                    event_tracks: serial.event_tracks,
                    source: Some(serial.source.clone()),
                    pending_gltf_source: Some(PendingGltfSource {
                        gltf_handle,
                        animation_name: animation_name.clone(),
                    }),
                }
            }
        };

        Ok(clip_mine)
    }

    fn extensions(&self) -> &[&str] {
        &["anim.ron"]
    }
}

impl TryFrom<&GraphClip> for GraphClipSerial {
    type Error = ();

    fn try_from(value: &GraphClip) -> Result<Self, Self::Error> {
        let Some(source) = value.source.clone() else {
            return Err(());
        };

        Ok(Self {
            source,
            skeleton: normalize_asset_path(value.skeleton.path().cloned().ok_or(())?),
            event_tracks: value.event_tracks.clone(),
        })
    }
}
