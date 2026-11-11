// The mod's row in Options > Mods, with its keys.
fn sky_lyrics_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use horizon_client_render::sky_lyrics as m;
    mods.push(crate::game::native_hooks::NativeMod {
        id: m::ID.into(),
        title: format!("{} {}", m::NAME, m::VERSION),
        description: m::DESCRIPTION.into(),
        keys: vec![
            [m::KEY_TOGGLE.into(), "Show / hide the lyrics".into(), m::DEFAULT_TOGGLE.into()],
            [m::KEY_MENU.into(), "Open the Sky Lyrics settings".into(), m::DEFAULT_MENU.into()],
        ],
    });
}
