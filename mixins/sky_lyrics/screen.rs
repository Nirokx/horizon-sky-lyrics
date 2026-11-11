// Draws the settings screen.
fn sky_lyrics_screen(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
) {
    use horizon_client_render::sky_lyrics as m;
    if game.native_mod_screen() != Some(m::ID) {
        return;
    }
    let size = [batch.width, batch.height];
    let measure = |t: &str| game.native_text_width(t);
    for draw in m::with(|s| s.screen(size, screen.mouse, &measure)) {
        match draw {
            m::Draw::Fill(rect, argb) => game.native_fill(batch, rect, argb),
            m::Draw::Text(text, at, rgb, shadow) => game.native_text(batch, &text, at, rgb, shadow),
        }
    }
}
