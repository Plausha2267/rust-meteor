fn meteor_chat_head(game: &mut crate::game::Game<'_>, message: &str) -> core::ops::ControlFlow<()> {
    match game.meteor_chat(message) {
        Some(()) => core::ops::ControlFlow::Break(()),
        None => core::ops::ControlFlow::Continue(()),
    }
}
