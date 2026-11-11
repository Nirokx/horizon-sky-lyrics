// While playing: J shows or hides the lyrics, K opens the settings screen.
fn sky_lyrics_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use horizon_client_render::sky_lyrics as m;
    if !pressed || !game.native_on(m::ID) || !game.native_playing() {
        return;
    }
    if key == game.native_mod_key(m::ID, m::KEY_TOGGLE, m::DEFAULT_TOGGLE) {
        m::with(|s| s.toggle());
    } else if key == game.native_mod_key(m::ID, m::KEY_MENU, m::DEFAULT_MENU) {
        game.native_mod_screen_open(m::ID);
    }
}
