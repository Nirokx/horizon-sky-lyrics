// A press on the settings screen.
fn sky_lyrics_mouse(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    use horizon_client_render::sky_lyrics as m;
    if game.native_mod_screen() != Some(m::ID) {
        return;
    }
    *canceled = true;
    if button == 0 {
        m::with(|s| s.press(screen.mouse));
    }
}
