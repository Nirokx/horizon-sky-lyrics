// Before each frame: the camera the sky is drawn from, and the colour the text
// can match, the sky's warmed by a sunrise or sunset when there is one.
fn sky_lyrics_render(renderer: &Renderer, frame: &Frame<'_>) {
    let [r, g, b, a] = frame.sky.sunrise_color;
    let base = frame.sky.sky_color;
    let k = (a * 0.7).clamp(0.0, 1.0);
    let sky = [base[0] + (r - base[0]) * k, base[1] + (g - base[1]) * k, base[2] + (b - base[2]) * k];
    let clip = renderer.views(frame).primary().view_proj(frame.bob * frame.spin).to_cols_array_2d();
    crate::sky_lyrics::prepare(&renderer.queue, clip, frame.yaw, frame.pitch, sky, frame.title);
}
