//! Post-processing effect configuration applied to a composited overlay.
//!
//! An [`EffectConfig`] pairs a [`EffectTarget`] (the whole screen or a sub-rect)
//! with [`EffectParams`] (which effect, and its tuning). The
//! [`post_processing_renderer`](super::post_processing_renderer) applies it. Every
//! params struct implements [`Default`], so start from the defaults and tweak only
//! the fields you care about.

/// Which post-processing effect a renderer should run. Used as a selector; the
/// per-effect tuning lives in [`EffectParams`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PostEffect {
    /// No effect; draw the source unchanged.
    #[default]
    Passthrough,
    /// Chromatic-aberration / scanline hologram look.
    Hologram,
    /// Animated warp-field / starfield background.
    WarpField,
    /// Glitch distortion (shares [`HologramParams`] tuning).
    Glitch,
}

/// Where an effect is applied.
#[derive(Clone, Copy, Debug, Default)]
pub enum EffectTarget {
    /// Apply across the whole render target.
    #[default]
    Fullscreen,
    /// Apply only inside the given `[x, y, width, height]` rectangle (pixels),
    /// for example to one Flutter widget.
    Widget([f32; 4]),
}

/// Tuning for the [`PostEffect::Hologram`] and [`PostEffect::Glitch`] effects.
#[derive(Clone, Copy, Debug)]
pub struct HologramParams {
    /// Strength of the RGB channel split. Default `0.005`.
    pub aberration_amount: f32,
    /// How fast the glitch/jitter animates. Default `10.0`.
    pub glitch_speed: f32,
    /// Visibility of the scanline overlay, `0.0` to `1.0`. Default `0.1`.
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

/// The selected effect together with its tuning parameters.
#[derive(Clone, Copy, Debug, Default)]
pub enum EffectParams {
    /// No effect (passthrough).
    #[default]
    None,
    /// Hologram effect with the given tuning.
    Hologram(HologramParams),
    /// Warp-field effect with the given tuning.
    WarpField(WarpFieldParams),
    /// Glitch effect; reuses [`HologramParams`].
    Glitch(HologramParams),
}

/// A complete post-processing description: what to draw and where.
#[derive(Clone, Copy, Debug, Default)]
pub struct EffectConfig {
    /// The region the effect covers.
    pub target: EffectTarget,
    /// The effect and its parameters.
    pub params: EffectParams,
}

/// Tuning for the [`PostEffect::WarpField`] effect (an animated starfield).
/// Defaults give a usable look; override individual fields as needed.
#[derive(Clone, Copy, Debug)]
pub struct WarpFieldParams {
    /// Animation speed of the field. Default `1.0`.
    pub speed: f32,
    /// Star density. Default `2.0`.
    pub density: f32,
    /// Base size of a star before glow. Default `0.003`.
    pub star_base_size: f32,
    /// How quickly the per-star glow falls off. Default `5.0`.
    pub glow_falloff: f32,
    /// Speed of the brightness pulse. Default `1.8`.
    pub pulse_speed: f32,
    /// Strength of the directional motion blur. Default `0.05`.
    pub motion_blur_strength: f32,
    /// Strength of the depth-based blur. Default `0.0005`.
    pub depth_blur_strength: f32,
    /// Overall opacity of the effect, `0.0` to `1.0`. Default `0.7`.
    pub base_alpha: f32,
    /// Inner color gradient stop, linear RGB. Default `[0.1, 0.2, 0.6]`.
    pub color_inner: [f32; 3],
    /// Outer color gradient stop, linear RGB. Default `[0.9, 0.1, 0.8]`.
    pub color_outer: [f32; 3],
    /// Color of the brightness pulse, linear RGB. Default `[1.0, 0.7, 0.0]`.
    pub color_pulse: [f32; 3],
    /// Brightness above which bloom kicks in. Default `0.5`.
    pub bloom_threshold: f32,
    /// Strength of the bloom. Default `0.8`.
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
