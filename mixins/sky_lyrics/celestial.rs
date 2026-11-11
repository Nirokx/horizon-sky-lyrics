// With no stars out the sky pass skips their step, where the lyrics are drawn:
// while the lyrics are on it always has some, at no brightness.
fn sky_lyrics_celestial(value: (Vec<Vertex>, Option<(glam::Mat4, f32)>)) -> (Vec<Vertex>, Option<(glam::Mat4, f32)>) {
    let (bodies, stars) = value;
    let stars = match stars {
        None if horizon_client_render::sky_lyrics::wants_sky() => Some((glam::Mat4::IDENTITY, 0.0)),
        other => other,
    };
    (bodies, stars)
}
