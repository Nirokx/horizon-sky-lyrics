// The current line in a corner of the HUD, when that setting is on.
fn sky_lyrics_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    use horizon_client::extras::Corner;
    use horizon_client_render::sky_lyrics as m;
    if !game.native_on(m::ID) || !game.native_window() {
        return;
    }
    let Some((text, corner)) = m::with(|s| s.hud_line()) else {
        return;
    };
    let corner = Corner::ALL[corner];
    if !game.native_corner_shown(corner) {
        return;
    }
    let width = game.native_text_width(&text);
    let (x, y) = corners.place(corner, width + 6.0, 12.0);
    game.native_fill(batch, [x, y, width + 6.0, 12.0], 0x78000000);
    game.native_text(batch, &text, [x + 3.0, y + 2.0], 0xFFFFFF, true);
}
