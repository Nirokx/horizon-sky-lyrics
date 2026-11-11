// A held slider follows the pointer.
fn sky_lyrics_drag(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    canceled: &mut bool,
) {
    use horizon_client_render::sky_lyrics as m;
    if game.native_mod_screen() != Some(m::ID) {
        return;
    }
    *canceled = true;
    m::with(|s| s.dragged(screen.mouse));
}
