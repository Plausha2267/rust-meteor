// Meteor Client for Horizon: the game adapter. Injected as
// `crate::game::meteor_client_adapter`; it moves data between the game and the
// engine in `crate::game::meteor_client` and holds no module logic itself.
use super::meteor_client::{Modules, PREFIX, Snapshot};
use super::{Game, KeyCode, Screen};
use std::sync::{Mutex, MutexGuard, OnceLock};

struct State {
    modules: Modules,
    /// Keys this adapter pressed on the player's behalf last tick.
    held_sprint: bool,
    held_jump: bool,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(|| {
            Mutex::new(State { modules: Modules::new(), held_sprint: false, held_jump: false })
        })
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Game<'_> {
    /// Runs before each `Game::tick`: let the active modules read the player
    /// and press keys / adjust motion before the game simulates the tick.
    pub(super) fn meteor_tick(&mut self) {
        let playing = self.screen == Screen::Playing && self.sleeping.is_none();
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let held = |k| playing && self.keys.contains(&k);
        let snapshot = Snapshot {
            on_ground: player.ground.on_ground,
            velocity: [player.velocity.x, player.velocity.y, player.velocity.z],
            fall_distance: player.fall_distance,
            yaw: player.yaw,
            forward: held(KeyCode::KeyW),
            backward: held(KeyCode::KeyS),
            left: held(KeyCode::KeyA),
            right: held(KeyCode::KeyD),
            jump: held(KeyCode::Space),
            sneak: held(KeyCode::ShiftLeft),
            playing,
        };
        let mut state = state();
        let actions = state.modules.tick(&snapshot);
        // Keys the player does not hold physically are ours to release.
        if actions.sprint {
            self.keys.insert(KeyCode::ControlLeft);
            state.held_sprint = true;
        } else if std::mem::take(&mut state.held_sprint) {
            self.keys.remove(&KeyCode::ControlLeft);
        }
        if actions.jump {
            self.keys.insert(KeyCode::Space);
            state.held_jump = true;
        } else if std::mem::take(&mut state.held_jump) {
            self.keys.remove(&KeyCode::Space);
        }
        if let Some(player) = self.player.as_mut() {
            if let Some(v) = actions.velocity {
                player.velocity.x = v[0];
                player.velocity.y = v[1];
                player.velocity.z = v[2];
            }
            if let Some(distance) = actions.fall_distance {
                player.fall_distance = distance;
            }
        }
    }

    /// `.command` typed in chat. `Some(())` when Meteor handled the line, which
    /// then never reaches the server.
    pub(super) fn meteor_chat(&mut self, message: &str) -> Option<()> {
        let line = message.trim().strip_prefix(PREFIX)?;
        self.meteor_run(line)
    }

    /// `/meteor <command>`.
    pub(super) fn meteor_slash(&mut self, command: &str) -> Option<bool> {
        let line = command.trim().strip_prefix("meteor")?;
        if !line.is_empty() && !line.starts_with(char::is_whitespace) {
            return None;
        }
        let line = if line.trim().is_empty() { "help" } else { line };
        self.meteor_run(line).map(|()| true)
    }

    fn meteor_run(&mut self, line: &str) -> Option<()> {
        let replies = state().modules.command(line)?;
        for reply in replies {
            self.system_message(horizon_client::text::Text::literal(format!("[Meteor] {reply}")));
        }
        Some(())
    }
}
