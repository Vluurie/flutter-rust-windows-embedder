#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PostEffect {
    #[default]
    Passthrough,
    Hologram,
    WarpField,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum EffectTarget {
    #[default]
    Fullscreen,

    Widget([f32; 4]), //make it widgets
}

#[derive(Clone, Copy, Debug)]
pub struct HologramParams {
    pub aberration_amount: f32,
    pub glitch_speed: f32,
    pub scanline_intensity: f32,
}

impl Default for HologramParams {
    fn default() -> Self {
        Self {
            aberration_amount: 0.005,
            glitch_speed: 10.0,
            scanline_intensity: 0.1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum EffectParams {
    #[default]
    None,
    Hologram(HologramParams),
    WarpField(WarpFieldParams),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EffectConfig {
    pub target: EffectTarget,
    pub params: EffectParams,
}

#[derive(Clone, Copy, Debug)]
pub struct WarpFieldParams {
    pub speed: f32,
    pub density: f32,
    pub star_base_size: f32,
    pub glow_falloff: f32,
    pub pulse_speed: f32,
    pub motion_blur_strength: f32,
    pub depth_blur_strength: f32,
    pub base_alpha: f32,
    pub color_inner: [f32; 3],
    pub color_outer: [f32; 3],
    pub color_pulse: [f32; 3],
    pub bloom_threshold: f32,
    pub bloom_intensity: f32,
}

impl Default for WarpFieldParams {
    fn default() -> Self {
        Self {
            speed: 1.0,
            density: 2.0,
            star_base_size: 0.003,
            glow_falloff: 5.0,
            pulse_speed: 1.8,
            motion_blur_strength: 0.05,
            depth_blur_strength: 0.0005,
            base_alpha: 0.7,
            color_inner: [0.1, 0.2, 0.6],
            color_outer: [0.9, 0.1, 0.8],
            color_pulse: [1.0, 0.7, 0.0],
            bloom_threshold: 0.5,
            bloom_intensity: 0.8,
        }
    }
}
