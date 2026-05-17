#![allow(dead_code)]

// bits 0..19: user defined (shown in editor UI)
pub const LAYER_DEFAULT: u32            = 1 << 0;

// bits 20..31: engine / editor reserved (hidden from UI)
pub const LAYER_EDITOR: u32             = 1 << 20;
pub const LAYER_QUAD_VIEW_TOP: u32      = 1 << 21;
pub const LAYER_QUAD_VIEW_FRONT: u32    = 1 << 22;
pub const LAYER_QUAD_VIEW_RIGHT: u32    = 1 << 23;
pub const LAYER_QUAD_VIEW_3D: u32       = 1 << 24;
// bits 25..31 reserved for future internal use

pub const LAYER_MASK_USER: u32          = 0x000F_FFFF; // bits 0..19
pub const LAYER_MASK_INTERNAL: u32      = 0xFFF0_0000; // bits 20..31
pub const LAYER_MASK_ALL: u32           = !0;

pub const LAYER_USER_FIRST_BIT: u32     = 0;
pub const LAYER_USER_COUNT: u32         = 20;