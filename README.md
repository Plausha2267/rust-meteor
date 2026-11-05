# Meteor Client for Horizon

A port of [Meteor Client](https://github.com/MeteorDevelopment/meteor-client) to
[Horizon](https://github.com/horizon-rust/horizon) as a **native mod** (a
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) bundle, built from
[horizon-native-mod-template](https://github.com/horizon-rust/horizon-native-mod-template)).
Horizon does not run Java jars, so this is a rewrite in Rust, not a loader. It contains no
Meteor source: it reimplements the documented behaviour of its modules, settings and commands.

## Status: early framework, not a full port

Meteor has about 190 modules, a click GUI, a HUD editor, waypoints, macros, profiles and
~950 Java files. This repository has the foundation and four modules.

| Part | State |
|---|---|
| Module registry, categories, typed settings (bool, int, double, choice) with validation | done, unit-tested |
| Commands: `help`, `modules`, `toggle`, `settings`, `set` (chat prefix `.` and `/meteor ...`) | done, unit-tested |
| Modules: Sprint, Flight (velocity/hover), NoFall, AutoJump | engine logic unit-tested |
| Game hooks (`Game::tick`, `send_chat`, `run_command`) | **written, not yet compiled against Horizon** |
| Click GUI, HUD, keybinds, config save/load | not started |
| Combat, render, world, most movement/player modules | not started |

"Not yet compiled against Horizon" is literal: the stage/check/build steps below have not been
run for this bundle, so the hooks and the `Game` field accesses in `adapter.rs` are unverified,
and none of it has been played in the game. Flight in particular overwrites the player's
velocity before the game's own tick applies gravity and drag, so its feel is untested.

## Layout

```text
mixins/meteor_client/
  horizon-mixin.toml  two support modules and three HEAD hooks, pinned to Nightly 2026-10-30
  runtime.rs          the engine: settings, modules, commands (pure Rust; also this crate's library)
  adapter.rs          reads the player, presses keys, sets motion, shows chat replies
  tick.rs chat.rs command.rs   the hook helpers
```

## Check the engine

```sh
cargo test
```

## Stage, build and play against Horizon

As in the template: clone `horizon` and `horizon-mixins` beside this repo, check out the
revisions in `compatibility.json`, then

```sh
python3 tools/horizon.py stage --horizon ../horizon
python3 tools/horizon.py check --horizon ../horizon
python3 tools/horizon.py test --horizon ../horizon
python3 tools/horizon.py build --horizon ../horizon
```

Use `.help` in chat, or `/meteor help`.

## Adding a module

Add a `Kind` variant, a `Module` entry in `Modules::new` with its settings, and its logic in
`Module::tick`, with a test that states the expected result independently of the code. A module
that needs more of the game than `Snapshot` and `Actions` carry extends those two types and
`meteor_tick` in `adapter.rs`.

License: MIT (see LICENSE). Meteor Client itself is GPL-3.0; this port copies none of its code.
