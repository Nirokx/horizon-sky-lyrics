// Once a frame: whether there is a world to draw in, messages for the chat,
// and the screen closing when the mod asked or the game closed it.
fn sky_lyrics_frame(game: &mut crate::game::Game<'_>) {
    use horizon_client_render::sky_lyrics as m;
    let active = game.native_on(m::ID) && game.native_player().is_some();
    let (close, notes) = m::with(|s| {
        s.set_active(active);
        (s.take_close(), s.take_notes())
    });
    for note in notes {
        game.native_chat(&note);
    }
    if close {
        game.native_mod_screen_close(m::ID);
    }
    let ours = game.native_mod_screen() == Some(m::ID);
    m::with(|s| s.sync_open(ours));
}
