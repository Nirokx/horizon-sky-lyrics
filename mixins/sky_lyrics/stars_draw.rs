// The stars' turn in the sky pass: the lyrics go first. The stars then set
// their own pipeline and bindings, and the pass restores its own after them.
fn sky_lyrics_stars_draw(pass: &mut wgpu::RenderPass<'_>) {
    crate::sky_lyrics::draw(pass);
}
