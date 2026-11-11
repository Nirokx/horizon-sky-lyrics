// A click on the page in Options > Mods.
fn sky_lyrics_click(mod_id: &str, button: usize) {
    use horizon_client_render::sky_lyrics as m;
    if mod_id != m::ID {
        return;
    }
    m::with(|s| match button {
        0 => s.toggle(),
        1 => s.reload(),
        2 => s.defaults(),
        _ => {}
    });
}
