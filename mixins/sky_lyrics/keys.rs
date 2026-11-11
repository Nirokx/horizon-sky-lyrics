// Keys on the settings screen: fields first. Otherwise the menu key closes it
// and the game keeps Escape.
fn sky_lyrics_keys(game: &crate::game::Game<'_>, key: &str, canceled: &mut bool) {
    use horizon_client_render::sky_lyrics as m;
    if game.native_mod_screen() != Some(m::ID) {
        return;
    }
    let menu = game.native_mod_key(m::ID, m::KEY_MENU, m::DEFAULT_MENU);
    m::with(|s| {
        if s.key(key) {
            *canceled = true;
        } else if key == menu {
            *canceled = true;
            s.close();
        }
    });
}
