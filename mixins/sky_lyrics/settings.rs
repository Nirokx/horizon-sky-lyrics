// The short page in Options > Mods; the full settings live on the K screen.
fn sky_lyrics_page(mod_id: &str, buttons: &mut Vec<String>) {
    use horizon_client_render::sky_lyrics as m;
    if mod_id != m::ID {
        return;
    }
    let shown = m::with(|s| s.shown());
    buttons.push(format!("Lyrics: {}", if shown { "On" } else { "Off" }));
    buttons.push("Reload the config file".to_owned());
    buttons.push("Reset every setting".to_owned());
    buttons.push("Everything else: press K in a world".to_owned());
}
