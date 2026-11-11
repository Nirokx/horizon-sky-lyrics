// A slider let go on the settings screen is saved.
fn sky_lyrics_release(game: &crate::game::Game<'_>, canceled: &mut bool) {
    use horizon_client_render::sky_lyrics as m;
    if game.native_mod_screen() != Some(m::ID) {
        return;
    }
    *canceled = true;
    m::with(|s| s.released());
}
