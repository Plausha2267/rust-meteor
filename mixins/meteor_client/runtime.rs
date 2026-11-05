// Meteor Client for Horizon: the engine.
//
// This file is both the standalone Cargo library and the injected
// `crate::game::meteor_client` module. It knows nothing about the game: modules
// read a `Snapshot` of the local player and answer with `Actions`; `adapter.rs`
// moves data between the game and these types. Original Rust implementation of
// the behaviour of Meteor Client's modules, settings and commands; no Meteor
// source is copied.

use std::fmt::Write as _;

/// Chat prefix of Meteor's commands (`Config.get().prefix`).
pub const PREFIX: &str = ".";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Combat,
    Player,
    Movement,
    Render,
    World,
    Misc,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Combat => "Combat",
            Category::Player => "Player",
            Category::Movement => "Movement",
            Category::Render => "Render",
            Category::World => "World",
            Category::Misc => "Misc",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int {
        value: i32,
        min: i32,
        max: i32,
    },
    Double {
        value: f64,
        min: f64,
        max: f64,
    },
    Choice {
        options: &'static [&'static str],
        index: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub name: &'static str,
    pub description: &'static str,
    pub value: Value,
}

impl Setting {
    pub fn bool(name: &'static str, description: &'static str, value: bool) -> Self {
        Self {
            name,
            description,
            value: Value::Bool(value),
        }
    }
    pub fn int(
        name: &'static str,
        description: &'static str,
        value: i32,
        min: i32,
        max: i32,
    ) -> Self {
        Self {
            name,
            description,
            value: Value::Int { value, min, max },
        }
    }
    pub fn double(
        name: &'static str,
        description: &'static str,
        value: f64,
        min: f64,
        max: f64,
    ) -> Self {
        Self {
            name,
            description,
            value: Value::Double { value, min, max },
        }
    }
    pub fn choice(
        name: &'static str,
        description: &'static str,
        options: &'static [&'static str],
        index: usize,
    ) -> Self {
        Self {
            name,
            description,
            value: Value::Choice { options, index },
        }
    }

    pub fn show(&self) -> String {
        match &self.value {
            Value::Bool(v) => v.to_string(),
            Value::Int { value, .. } => value.to_string(),
            Value::Double { value, .. } => format!("{value}"),
            Value::Choice { options, index } => options[*index].to_owned(),
        }
    }

    /// Parse and store a value the way Meteor's settings do: out-of-range
    /// numbers are rejected, choices match case-insensitively.
    pub fn set_from_str(&mut self, text: &str) -> Result<(), String> {
        match &mut self.value {
            Value::Bool(v) => {
                *v = match text.to_ascii_lowercase().as_str() {
                    "true" | "on" | "yes" => true,
                    "false" | "off" | "no" => false,
                    _ => return Err(format!("'{text}' is not true or false")),
                }
            }
            Value::Int { value, min, max } => {
                let n: i32 = text
                    .parse()
                    .map_err(|_| format!("'{text}' is not a whole number"))?;
                if n < *min || n > *max {
                    return Err(format!("{n} is outside {min}..{max}"));
                }
                *value = n;
            }
            Value::Double { value, min, max } => {
                let n: f64 = text
                    .parse()
                    .map_err(|_| format!("'{text}' is not a number"))?;
                if !n.is_finite() || n < *min || n > *max {
                    return Err(format!("{text} is outside {min}..{max}"));
                }
                *value = n;
            }
            Value::Choice { options, index } => {
                *index = options
                    .iter()
                    .position(|o| o.eq_ignore_ascii_case(text))
                    .ok_or_else(|| format!("choose one of: {}", options.join(", ")))?;
            }
        }
        Ok(())
    }
}

/// What a module may read of the local player each tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub on_ground: bool,
    pub velocity: [f64; 3],
    pub fall_distance: f64,
    /// Yaw in degrees (0 = south, as the game stores it).
    pub yaw: f32,
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sneak: bool,
    /// The world is being played: no screen is open.
    pub playing: bool,
}

impl Snapshot {
    pub fn moving(&self) -> bool {
        self.forward || self.backward || self.left || self.right
    }
}

/// What modules ask the adapter to do this tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Actions {
    /// Hold the sprint key down while moving forward.
    pub sprint: bool,
    pub velocity: Option<[f64; 3]>,
    pub fall_distance: Option<f64>,
    pub jump: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Sprint,
    Flight,
    NoFall,
    AutoJump,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub category: Category,
    pub kind: Kind,
    pub active: bool,
    pub settings: Vec<Setting>,
}

impl Module {
    pub fn setting(&self, name: &str) -> Option<&Setting> {
        self.settings
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }
    fn flag(&self, name: &str) -> bool {
        matches!(
            self.setting(name).map(|s| &s.value),
            Some(Value::Bool(true))
        )
    }
    fn number(&self, name: &str) -> f64 {
        match self.setting(name).map(|s| &s.value) {
            Some(Value::Double { value, .. }) => *value,
            Some(Value::Int { value, .. }) => f64::from(*value),
            _ => 0.0,
        }
    }
    fn choice(&self, name: &str) -> usize {
        match self.setting(name).map(|s| &s.value) {
            Some(Value::Choice { index, .. }) => *index,
            _ => 0,
        }
    }

    fn tick(&self, s: &Snapshot, out: &mut Actions) {
        match self.kind {
            Kind::Sprint => {
                // Meteor's Sprint: always sprint while moving forward; the
                // "when stationary" option also sprints when standing still.
                if s.forward && !s.sneak || (self.flag("when-stationary") && !s.moving()) {
                    out.sprint = true;
                }
                if s.moving() && self.flag("all-directions") && !s.sneak {
                    out.sprint = true;
                }
            }
            Kind::NoFall => {
                // The "always" mode of Meteor's NoFall: never accumulate fall
                // distance, so landing deals no damage.
                if s.fall_distance > 0.0 {
                    out.fall_distance = Some(0.0);
                }
            }
            Kind::AutoJump => {
                if s.on_ground
                    && s.moving()
                    && !s.sneak
                    && (!self.flag("only-forward") || s.forward)
                {
                    out.jump = true;
                }
            }
            Kind::Flight => {
                let speed = self.number("speed");
                let vertical = self.number("vertical-speed");
                let mut v = s.velocity;
                if self.choice("mode") == 0 {
                    // "Velocity": steer in the air by overwriting the motion.
                    let (dx, dz) = horizontal(s, speed);
                    v[0] = dx;
                    v[2] = dz;
                    v[1] = match (s.jump, s.sneak) {
                        (true, false) => vertical,
                        (false, true) if !self.flag("no-sneak") => -vertical,
                        _ => 0.0,
                    };
                } else {
                    // "Hover": keep height, only horizontal control.
                    let (dx, dz) = horizontal(s, speed);
                    v[0] = dx;
                    v[2] = dz;
                    if v[1] < 0.0 {
                        v[1] = 0.0;
                    }
                    if s.jump {
                        v[1] = vertical;
                    }
                }
                out.velocity = Some(v);
                out.fall_distance = Some(0.0);
            }
        }
    }
}

/// Horizontal velocity for the held movement keys at `speed` blocks per tick.
pub fn horizontal(s: &Snapshot, speed: f64) -> (f64, f64) {
    let forward = f64::from(i32::from(s.forward) - i32::from(s.backward));
    let strafe = f64::from(i32::from(s.left) - i32::from(s.right));
    if forward == 0.0 && strafe == 0.0 {
        return (0.0, 0.0);
    }
    let len = (forward * forward + strafe * strafe).sqrt();
    let (forward, strafe) = (forward / len, strafe / len);
    // yaw 0 faces +Z, positive yaw turns toward -X.
    let rad = f64::from(s.yaw).to_radians();
    let (sin, cos) = rad.sin_cos();
    (
        (strafe * cos - forward * sin) * speed,
        (forward * cos + strafe * sin) * speed,
    )
}

#[derive(Debug)]
pub struct Modules {
    pub list: Vec<Module>,
}

impl Default for Modules {
    fn default() -> Self {
        Self::new()
    }
}

impl Modules {
    pub fn new() -> Self {
        let list = vec![
            Module {
                name: "sprint",
                title: "Sprint",
                description: "Automatically sprints.",
                category: Category::Movement,
                kind: Kind::Sprint,
                active: false,
                settings: vec![
                    Setting::bool(
                        "all-directions",
                        "Sprint in every direction, not only forward.",
                        false,
                    ),
                    Setting::bool("when-stationary", "Sprint while standing still.", false),
                ],
            },
            Module {
                name: "flight",
                title: "Flight",
                description: "FLYYYY! No fall damage while it is on.",
                category: Category::Movement,
                kind: Kind::Flight,
                active: false,
                settings: vec![
                    Setting::choice("mode", "How flight is applied.", &["velocity", "hover"], 0),
                    Setting::double(
                        "speed",
                        "Horizontal speed in blocks per tick.",
                        0.1,
                        0.0,
                        10.0,
                    ),
                    Setting::double(
                        "vertical-speed",
                        "Climb and descent speed in blocks per tick.",
                        0.1,
                        0.0,
                        10.0,
                    ),
                    Setting::bool(
                        "no-sneak",
                        "Hold height while sneaking instead of descending.",
                        false,
                    ),
                ],
            },
            Module {
                name: "no-fall",
                title: "NoFall",
                description: "Prevents fall damage.",
                category: Category::Player,
                kind: Kind::NoFall,
                active: false,
                settings: vec![],
            },
            Module {
                name: "auto-jump",
                title: "AutoJump",
                description: "Automatically jumps while you move.",
                category: Category::Movement,
                kind: Kind::AutoJump,
                active: false,
                settings: vec![Setting::bool(
                    "only-forward",
                    "Only jump when moving forward.",
                    false,
                )],
            },
        ];
        Self { list }
    }

    pub fn find(&self, name: &str) -> Option<&Module> {
        self.list
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name) || m.title.eq_ignore_ascii_case(name))
    }
    pub fn find_mut(&mut self, name: &str) -> Option<&mut Module> {
        self.list
            .iter_mut()
            .find(|m| m.name.eq_ignore_ascii_case(name) || m.title.eq_ignore_ascii_case(name))
    }

    /// Run every active module and merge what they ask for. Later modules in
    /// the list win a conflict; `jump` and `sprint` are OR-ed.
    pub fn tick(&self, snapshot: &Snapshot) -> Actions {
        let mut out = Actions::default();
        if !snapshot.playing {
            return out;
        }
        for module in self.list.iter().filter(|m| m.active) {
            let mut mine = Actions::default();
            module.tick(snapshot, &mut mine);
            out.sprint |= mine.sprint;
            out.jump |= mine.jump;
            if mine.velocity.is_some() {
                out.velocity = mine.velocity;
            }
            if mine.fall_distance.is_some() {
                out.fall_distance = mine.fall_distance;
            }
        }
        out
    }

    /// Run one Meteor command (without its prefix). `None` when `line` names
    /// no command, so the game can handle it as ordinary chat.
    pub fn command(&mut self, line: &str) -> Option<Vec<String>> {
        let mut words = line.split_whitespace();
        let name = words.next()?.to_ascii_lowercase();
        let args: Vec<&str> = words.collect();
        let reply = match (name.as_str(), args.as_slice()) {
            ("help", []) => vec![
                format!("{PREFIX}toggle <module> [on|off]  switch a module"),
                format!("{PREFIX}modules                  list modules by category"),
                format!("{PREFIX}settings <module>        show a module's settings"),
                format!("{PREFIX}set <module> <setting> <value>"),
            ],
            ("modules" | "m", []) => self.list_modules(),
            ("toggle" | "t", [module]) => self.toggle(module, None),
            ("toggle" | "t", [module, state]) => match state.to_ascii_lowercase().as_str() {
                "on" => self.toggle(module, Some(true)),
                "off" => self.toggle(module, Some(false)),
                _ => vec![format!("Expected on or off, not '{state}'.")],
            },
            ("settings" | "s", [module]) => match self.find(module) {
                None => vec![format!("No module named '{module}'.")],
                Some(m) if m.settings.is_empty() => vec![format!("{} has no settings.", m.title)],
                Some(m) => m
                    .settings
                    .iter()
                    .map(|s| format!("{} = {}  ({})", s.name, s.show(), s.description))
                    .collect(),
            },
            ("set", [module, setting, value]) => match self.find_mut(module) {
                None => vec![format!("No module named '{module}'.")],
                Some(m) => {
                    let title = m.title;
                    match m
                        .settings
                        .iter_mut()
                        .find(|s| s.name.eq_ignore_ascii_case(setting))
                    {
                        None => vec![format!("{title} has no setting '{setting}'.")],
                        Some(s) => match s.set_from_str(value) {
                            Ok(()) => vec![format!("{title}: {} set to {}.", s.name, s.show())],
                            Err(e) => vec![e],
                        },
                    }
                }
            },
            ("help" | "modules" | "m" | "toggle" | "t" | "settings" | "s" | "set", _) => {
                vec![format!("Wrong arguments. Try {PREFIX}help.")]
            }
            _ => return None,
        };
        Some(reply)
    }

    fn toggle(&mut self, module: &str, state: Option<bool>) -> Vec<String> {
        match self.find_mut(module) {
            None => vec![format!("No module named '{module}'.")],
            Some(m) => {
                m.active = state.unwrap_or(!m.active);
                vec![format!(
                    "Toggled {} {}.",
                    m.title,
                    if m.active { "on" } else { "off" }
                )]
            }
        }
    }

    fn list_modules(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for category in [
            Category::Combat,
            Category::Player,
            Category::Movement,
            Category::Render,
            Category::World,
            Category::Misc,
        ] {
            let mut line = String::new();
            for m in self.list.iter().filter(|m| m.category == category) {
                if !line.is_empty() {
                    line.push_str(", ");
                }
                let _ = write!(line, "{}{}", m.title, if m.active { " (on)" } else { "" });
            }
            if !line.is_empty() {
                lines.push(format!("{}: {line}", category.name()));
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing() -> Snapshot {
        Snapshot {
            playing: true,
            ..Snapshot::default()
        }
    }

    #[test]
    fn inactive_modules_do_nothing() {
        let m = Modules::new();
        let s = Snapshot {
            forward: true,
            fall_distance: 9.0,
            ..playing()
        };
        assert_eq!(m.tick(&s), Actions::default());
    }

    #[test]
    fn nothing_runs_while_a_screen_is_open() {
        let mut m = Modules::new();
        m.command("toggle sprint on");
        let s = Snapshot {
            forward: true,
            playing: false,
            ..Snapshot::default()
        };
        assert!(!m.tick(&s).sprint);
    }

    #[test]
    fn sprint_needs_forward_unless_configured() {
        let mut m = Modules::new();
        m.command("t sprint on");
        let strafe = Snapshot {
            left: true,
            ..playing()
        };
        assert!(!m.tick(&strafe).sprint);
        assert!(
            m.tick(&Snapshot {
                forward: true,
                ..playing()
            })
            .sprint
        );
        assert!(
            !m.tick(&Snapshot {
                forward: true,
                sneak: true,
                ..playing()
            })
            .sprint
        );
        m.command("set sprint all-directions true");
        assert!(m.tick(&strafe).sprint);
    }

    #[test]
    fn no_fall_clears_only_real_falls() {
        let mut m = Modules::new();
        m.command("toggle no-fall on");
        assert_eq!(m.tick(&playing()).fall_distance, None);
        let falling = Snapshot {
            fall_distance: 6.5,
            ..playing()
        };
        assert_eq!(m.tick(&falling).fall_distance, Some(0.0));
    }

    #[test]
    fn auto_jump_waits_for_the_ground() {
        let mut m = Modules::new();
        m.command("toggle autojump on");
        let air = Snapshot {
            forward: true,
            ..playing()
        };
        assert!(!m.tick(&air).jump);
        assert!(
            m.tick(&Snapshot {
                on_ground: true,
                ..air.clone()
            })
            .jump
        );
        m.command("set auto-jump only-forward true");
        let strafe = Snapshot {
            on_ground: true,
            left: true,
            ..playing()
        };
        assert!(!m.tick(&strafe).jump);
    }

    #[test]
    fn flight_moves_along_the_facing_direction() {
        // Facing +Z (yaw 0), forward at 0.5 blocks per tick.
        let (x, z) = horizontal(
            &Snapshot {
                forward: true,
                ..playing()
            },
            0.5,
        );
        assert!(x.abs() < 1e-12 && (z - 0.5).abs() < 1e-12);
        // Yaw 90 faces -X.
        let east = Snapshot {
            forward: true,
            yaw: 90.0,
            ..playing()
        };
        let (x, z) = horizontal(&east, 1.0);
        assert!((x + 1.0).abs() < 1e-12 && z.abs() < 1e-12);
        // Diagonals are not faster than straight lines.
        let (x, z) = horizontal(
            &Snapshot {
                forward: true,
                left: true,
                ..playing()
            },
            1.0,
        );
        assert!(((x * x + z * z).sqrt() - 1.0).abs() < 1e-12);
        assert_eq!(horizontal(&playing(), 1.0), (0.0, 0.0));
    }

    #[test]
    fn flight_climbs_descends_and_resets_fall_distance() {
        let mut m = Modules::new();
        m.command("toggle flight on");
        m.command("set flight vertical-speed 0.25");
        let up = m.tick(&Snapshot {
            jump: true,
            velocity: [0.0, -3.0, 0.0],
            fall_distance: 8.0,
            ..playing()
        });
        assert_eq!(up.velocity.map(|v| v[1]), Some(0.25));
        assert_eq!(up.fall_distance, Some(0.0));
        let down = m.tick(&Snapshot {
            sneak: true,
            ..playing()
        });
        assert_eq!(down.velocity.map(|v| v[1]), Some(-0.25));
        let level = m.tick(&playing());
        assert_eq!(level.velocity.map(|v| v[1]), Some(0.0));
    }

    #[test]
    fn settings_reject_bad_values_and_keep_the_old_one() {
        let mut m = Modules::new();
        let r = m.command("set flight speed 99").unwrap();
        assert!(r[0].contains("outside"), "{r:?}");
        assert_eq!(
            m.find("flight").unwrap().setting("speed").unwrap().show(),
            "0.1"
        );
        let r = m.command("set flight mode nonsense").unwrap();
        assert!(r[0].contains("velocity, hover"), "{r:?}");
        m.command("set flight mode HOVER");
        assert_eq!(
            m.find("flight").unwrap().setting("mode").unwrap().show(),
            "hover"
        );
        assert!(m.command("set flight speed abc").unwrap()[0].contains("not a number"));
        assert!(m.command("set nothing x y").unwrap()[0].contains("No module"));
    }

    #[test]
    fn commands_ignore_ordinary_chat() {
        let mut m = Modules::new();
        assert_eq!(m.command("hello everyone"), None);
        assert_eq!(m.command(""), None);
        assert!(m.command("toggle").unwrap()[0].contains("Wrong arguments"));
        assert!(m.command("toggle nope").unwrap()[0].contains("No module"));
    }

    #[test]
    fn toggle_flips_and_modules_lists_state() {
        let mut m = Modules::new();
        assert_eq!(m.command("toggle sprint").unwrap(), ["Toggled Sprint on."]);
        assert_eq!(
            m.command("toggle sprint off").unwrap(),
            ["Toggled Sprint off."]
        );
        m.command("toggle no-fall on");
        let list = m.command("modules").unwrap();
        assert!(list.contains(&"Player: NoFall (on)".to_owned()), "{list:?}");
        assert!(
            list.iter()
                .any(|l| l.starts_with("Movement: ") && l.contains("Flight"))
        );
        assert!(m.command("toggle sprint maybe").unwrap()[0].contains("on or off"));
    }
}
