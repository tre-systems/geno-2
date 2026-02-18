/// Frame smoothing and interaction tuning constants.
///
/// These constants express intended behavior (e.g., time constants, clamp
/// limits) and keep magic numbers out of the code, improving readability.
use glam::Vec3;

// Exponential decay rate for internal pulse energy
pub const PULSE_ENERGY_DECAY_PER_SEC: f32 = 1.28;

// Target smoothing time constants (seconds)
pub const PULSE_RISE_TAU_SEC: f32 = 0.052;
pub const PULSE_FALL_TAU_SEC: f32 = 0.48;

// Pointer speed clamp (normalized units per second)
pub const POINTER_SPEED_MAX: f32 = 10.0;

// Inertial swirl spring parameters
pub const SWIRL_OMEGA: f32 = 1.45; // natural frequency
pub const SWIRL_DAMPING_RATIO: f32 = 0.38; // 0..1 critical at 1
pub const SWIRL_MAX_STEP_PER_SEC: f32 = 0.72; // cap motion per second (in uv units)

// Swirl energy blend weights
pub const SWIRL_TARGET_WEIGHT_POINTER: f32 = 0.32;
pub const SWIRL_TARGET_WEIGHT_VELOCITY: f32 = 0.42;
pub const SWIRL_TARGET_CLICK_BONUS: f32 = 0.35;
pub const SWIRL_ENERGY_BLEND_ALPHA: f32 = 0.18; // new = (1-α)*old + α*target

// Global FX mapping weights
pub const FX_REVERB_BASE: f32 = 0.16;
pub const FX_REVERB_SPAN: f32 = 0.44;

pub const FX_DELAY_WET_BASE: f32 = 0.08;
pub const FX_DELAY_WET_SWIRL: f32 = 0.40;
pub const FX_DELAY_WET_ECHO: f32 = 0.26;

pub const FX_DELAY_FB_BASE: f32 = 0.28;
pub const FX_DELAY_FB_SWIRL: f32 = 0.32;
pub const FX_DELAY_FB_ECHO: f32 = 0.19;

pub const FX_SAT_DRIVE_MIN: f32 = 0.24;
pub const FX_SAT_DRIVE_MAX: f32 = 2.10;
pub const FX_SAT_WET_BASE: f32 = 0.10;
pub const FX_SAT_WET_SPAN: f32 = 0.64;

// Visual build parameters

// Per-voice spatial sends mapping
pub const DIST_NORM_DIVISOR: f32 = 2.7;
pub const D_SEND_BASE: f32 = 0.08;
pub const D_SEND_SPAN: f32 = 0.56;
pub const R_SEND_BASE: f32 = 0.16;
pub const R_SEND_SPAN: f32 = 0.52;
pub const SEND_BOOST_COEFF: f32 = 0.52;
pub const D_SEND_CLAMP_MAX: f32 = 1.2;
pub const R_SEND_CLAMP_MAX: f32 = 1.5;

// Voice level mapping
pub const LEVEL_BASE: f32 = 0.58;
pub const LEVEL_SPAN: f32 = 0.46;

// Color adjustments

// Camera
// Z distance used by both picking and audio listener alignment.
pub const CAMERA_Z: f32 = 6.0;

// Voice interaction
pub const PICK_SPHERE_RADIUS: f32 = 0.5;
pub const SPREAD: Vec3 = glam::Vec3::new(3.0, 3.0, 3.0);
pub const Z_OFFSET: Vec3 = glam::Vec3::new(0.0, 0.0, -1.5);
pub const ENGINE_DRAG_MAX_RADIUS: f32 = 1.0;

// Post-processing defaults
pub const BLOOM_STRENGTH: f32 = 0.68;
pub const BLOOM_THRESHOLD: f32 = 0.68;
