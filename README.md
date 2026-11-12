_Library crate:_
[![Crates.io](https://img.shields.io/crates/v/bevy_animation_graph)](https://crates.io/crates/bevy_animation_graph)
[![Crates.io](https://img.shields.io/crates/d/bevy_animation_graph)](https://crates.io/crates/bevy_animation_graph)

_Editor crate:_
[![Crates.io](https://img.shields.io/crates/v/bevy_animation_graph_editor)](https://crates.io/crates/bevy_animation_graph_editor)
[![Crates.io](https://img.shields.io/crates/d/bevy_animation_graph_editor)](https://crates.io/crates/bevy_animation_graph_editor)

[![CI](https://github.com/mbrea-c/bevy_animation_graph/actions/workflows/ci.yaml/badge.svg)](https://github.com/mbrea-c/bevy_animation_graph/actions/workflows/ci.yaml)

# Bevy Animation Graph

> **_NOTICE:_** If you're currently using version `0.8.0` or lower, note that
> `0.9.0` has introduced a number of breaking changes. Check out the
> [migration guide](https://mbrea-c.github.io/bevy_animation_graph/migration_guides/0.8_to_0.9.html)
> for upgrading.

## Fork notes (jkukatzki/bevy_animation_graph)

This branch tracks upstream `master` (up to #153) and adds the following.

### Differences from upstream

- **Bevy 0.20** (and the matching `bevy_egui`).
- **Deferred glTF clip resolution:** `GltfNamed` graph clips no longer block on
  the glTF load. `resolve_pending_graph_clips` fills in their curves once the
  glTF and its animation sub-assets are ready, retrying on any asset change.
  `ClipNode` keeps the pose's skeleton while the clip is still pending.
- **`GraphNodeResources`:** a type map that apps fill with their own data
  (e.g. lookup tables) so custom nodes can read it during graph evaluation.
- **Timed state transitions on demand:** `AnimationEvent::TransitionToStateLabelOver`
  overrides a timed transition's duration; `FsmState::transition_duration`
  stores it.
- **Runtime transition source** (see below): `FsmState::transition_source`.
- **Clip sampling** uses `sample_clamped` (`GraphClip::sample_clamped`).
- **`SpeedNode` timestamp guard:** below `SpeedNode::MIN_TIMESTAMP_SPEED`
  (1e-3) the output timestamp is no longer divided by the speed. Near-zero
  speeds produced timestamps so large that f32 could not represent frame-sized
  steps, which froze anything synced to them (e.g. an `Absolute`-synced blend).
- **`AnimationUpdateRate`:** an optional component that evaluates a player's
  graph every `interval` seconds (staggered by `phase`) instead of every
  frame, e.g. for distant characters. Deltas accumulate between updates;
  skipped frames skip graph evaluation, the entity map and bone writes, and
  report no root motion. `interpolate` blends bones between the last two
  evaluated poses on the frames in between.
- **Cached entity map:** rebuilt only when `Children` or `Name` change on the
  root or a mapped entity, not every frame.
- **`apply_animation_to_targets`** writes bone transforms with `set_if_neq`,
  so unchanged bones keep their change ticks.
- **Editor:** responsive inspector columns, optional max popup width,
  Cmd+Backspace to delete on macOS, a fix for a `DragValue` panic on
  non-finite values, and the node inspector scoped to the graph asset.

### Why the transition-source fix was needed

A state's `state_transition` is a transition from *any* state into that state.
The high-level → low-level FSM conversion creates **one** low-level transition
state (`HlTransition(TransitionId::State(target))`) and one start transition
(`Start(TransitionId::State(target))`) per target. It does that once for every
possible source, though, and each pass overwrote the previous one under the
same id. The stored `LlTransitionData::source` and `LowLevelTransition::hl_source`
therefore ended up as whichever source the `HashMap` visited last.

`FromFsmSource` pins resolved through that static source. So an "any → jump"
blend did not blend from the state you were actually in (e.g. `idle`). It
blended from an arbitrary state, `float` in our case, and that state's pose
flashed for the first frames of the blend. Explicit (direct) transitions were
not affected, because they have one id per source/target pair.

### How the fix changes things

- `FsmState` gains `transition_source: Option<StateId>`. When a transition into
  a transition state fires, it records the high-level state the FSM is
  actually leaving. If the FSM is already mid-transition, it falls back to the
  transition's `hl_source`.
- `FsmIoEnv` carries the active transition state and its runtime source.
  `FromFsmSource` data, duration and time lookups for that transition use it
  instead of the static source. Nested state lookups keep the old behaviour.
- State transitions now blend from the real previous state's pose. Direct
  transitions behave exactly as before.
- **Breaking for code that builds `FsmState` by hand:** add
  `transition_source: None` (or the correct source when starting a transition
  manually).

## Motivation

Animation graphs are an essential tool for managing the complexity present in
the animation pipelines for modern 3D games. When your game has tens of
animations with complex blends and transitions, or you want to generate your
animations procedurally from very few keyframes, simple animation clip playback
is not enough.

This crate works as an alternative to most of `bevy_animation`, and aims to
provide a complete animation system and development workflow including animation
graphs, animation state machines (i.e. animation controllers) and a graphical
editor to create your animation graphs and state machines.

_NOTE: This project is separate from the animation graphs introduced in
`bevy_animation` in version 0.14._

![example usage screenshot](locomotion_graph.png)

## Current Features

- Animation graphs are assets. They can be loaded from asset files, or created
  in code with an ergonomic API.
- Visual graph editor.
- Animation state machines:
  - Animation state machines are embedded as nodes in animation graphs.
  - Each state in a state machines plays back its own animation graph on demand.
  - Transitions also have their own animation graph, and they can query source
    and target states' animation graphs.
- Ragdoll support:
  - Visual ragdoll editing in the editor.
  - Support for _partial ragdolls_, where some bones are simulated and others
    kinematically driven by a target animation pose.
- Support for custom nodes written in Rust.
  - Users can create their own editor binary by adding the editor plugin and
    registering their additional custom nodes in the Bevy type registry. The
    editor will automatically pick up all registered nodes.
- Available built-in nodes include:
  - Animation chaining (i.e. play one node after another).
  - Two-bone inverse kinematics.
  - Looping.
  - Linear Blending (in bone space).
  - 2D Blend space node.
  - Mirror animation about the YZ plane.
  - Animation clip playback.
  - Apply a given rotation to some bones in a pose using a bone mask.
  - Speed up or slow down animation playback.
  - Animation graph node.
  - Animation state machine node.
  - Arithmetic nodes: a variety of common operations on f32, Vec3, Quat, etc.
- Nesting animation graphs as nodes within other graphs.
- Animation synchronization using event tracks.
- Output from graph nodes is cached to avoid unnecessary computations.

## Planned Features

1. Ragdoll "pose following" modes, where a simulated ragdoll bones try to match
   a target pose using physical forces:
   - _Absolute_ matching mode, where the world space position/rotation of the
     target bone is tracked and use as an anchor for a spring force on the
     ragdoll bone.
   - _Relative_ matching mode, where the ragdoll bone tries to match the
     target's position/rotation relative to its parent bone. We may need joint
     motors support on Avian to implement this mode properly.

## Installation

### Library

[bevy_animation_graph](https://crates.io/crates/bevy_animation_graph) is the
library part of this project. This should be added as a dependency to your
project in order to use animation graphs. To install the latest published
version from crates.io run

```bash
cargo add bevy_animation_graph
```

or manually add the latest version to your `Cargo.toml`.

To install the latest git master, add the following to `Cargo.toml`:

```toml
# ...
[dependencies]
# ...
bevy_animation_graph = { git = "https://github.com/mbrea-c/bevy_animation_graph.git" }
# ...
```

### Editor

[bevy_animation_graph_editor](https://crates.io/crates/bevy_animation_graph_editor)
is the editor. You can install like you would install any other rust binary:

```bash
# for the latest crates.io version
cargo install bevy_animation_graph_editor
# for the latest master
cargo install --git https://github.com/mbrea-c/bevy_animation_graph bevy_animation_graph_editor
# for the version from a local workspace
cargo install --path <PATH_TO_WORKSPACE> bevy_animation_graph_editor

# use the --force flag to force reinstall
```

Alternatively, I recommend taking a look at the
[editor as a plugin example](https://github.com/mbrea-c/bevy_animation_graph/blob/master/examples/editor_as_a_plugin/examples/editor_as_a_plugin.rs)
in order to set up your own editor binary in your crate/workspace, consuming the
same library version as your game. This will let you register custom animation
graph nodes, which are a powerful feature of this crate.

## Getting started

Check out the tutorials in the
[work in progress book](https://sarahihme.github.io/initial_book_bevy_animation_graph/)
(many thanks to @SarahIhme for getting this started!).

Additional, up to date examples are included in the
[examples](https://github.com/mbrea-c/bevy_animation_graph/tree/master/examples)
directory.

The following resources may also be useful, but they are somewhat out of date:

- The documentation in [docs.rs](https://docs.rs/bevy_animation_graph) contains
  an introduction of the library and editor and an explanation of a simple
  animation graph example. See also the video below for a demonstration of
  editor usage.
- There is an outdated
  [usage video walkthrough](https://www.youtube.com/watch?v=q-JBSQJIcX0).

## Contributing or getting help

If you run into a bug or want to discuss potential new features, feel free to
post an issue, open a PR or reach out to me in Discord (@mbreac in the Bevy
discord, there's a crate-help post for `bevy_animation_graph` there:
[link](https://discord.com/channels/691052431525675048/1202998277482479616)).

## FAQ

### Is this ready for production?

Depends.

It can already be useful for small-ish projects, but I cannot guarantee API
stability between different `0.x` versions (it is a big library, it is
relatively young and I don't have previous experience with animation
programming, so I'm still figuring out the best ways of doing things). This
means that it will likely be necessary to go into your animation graph assets
and manually migrate them between versions, at least until I find a better way
to handle migrations.

Additionally, there ~~may~~ will be bugs and other issues. I try to get them
fixed as they come up.

### Will you implement feature X?

If it's a small feature (e.g. some additional vector or floating point
arithmetic node) it's likely that I have just not got around to it. If you open
an issue I will probably implement it quickly. PRs are also welcome.

For larger features, it's better to start by opening an issue for discussion or
pinging me in the Bevy discord.

## Version table

| `bevy` | `bevy_animation_graph` | 
| ------ | ---------------------- | 
| 0.19 | master | 
| 0.19 | 0.11 | 
| 0.18 | 0.10 | 
| 0.17 | 0.9 | 
| 0.17 | 0.8 | 
| 0.16 | 0.7 | 
| 0.15 | 0.6 | 
| 0.14 | 0.5 | 
| 0.13 | 0.4 | 
| 0.13 | 0.3 | 
| 0.12 | 0.2 |

## Acknowledgements

Many thanks to [Bobby Anguelov](https://www.youtube.com/@BobbyAnguelov) for his
lectures on animation programming.
