// Letters typed into a colour field or the font search.
fn sky_lyrics_chars(game: &crate::game::Game<'_>, c: char, canceled: &mut bool) {
    use horizon_client_render::sky_lyrics as m;
    if game.native_mod_screen() != Some(m::ID) {
        return;
    }
    *canceled = true;
    m::with(|s| s.typed(c));
}
