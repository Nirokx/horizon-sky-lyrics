// Made once per renderer, with the device and the format the sky is drawn in.
fn sky_lyrics_stars_new(device: &wgpu::Device, format: wgpu::TextureFormat) {
    crate::sky_lyrics::renderer_made(device, format);
}
