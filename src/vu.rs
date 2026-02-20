// VU1 (Vector Unit 1) stub.
// Maps to: the VU1 300 MHz SIMD co-processor running a micro-program that
// transforms vertices in VU data memory and XGKICK-sends GIF tags to the GS.
// Our execute_micro_program() is that entire micro-program in Rust.

use crate::gs::GsVertex;
use std::f32::consts::PI;

// ---------------------------------------------------------------------------
// Core math types
// ---------------------------------------------------------------------------

/// Column-major 4×4 matrix.
/// Maps to: VU1 has 32 × 128-bit VF registers; a mat4 occupies 4 of them.
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

/// 4-component float vector.
/// Maps to: a single VU1 VF register.
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn rotate_y(rad: f32) -> Mat4 {
        let (s, c) = (rad.sin(), rad.cos());
        Mat4 {
            cols: [
                [ c,   0.0, -s,  0.0],
                [ 0.0, 1.0,  0.0, 0.0],
                [ s,   0.0,  c,  0.0],
                [ 0.0, 0.0,  0.0, 1.0],
            ],
        }
    }

    pub fn rotate_x(rad: f32) -> Mat4 {
        let (s, c) = (rad.sin(), rad.cos());
        Mat4 {
            cols: [
                [1.0,  0.0, 0.0, 0.0],
                [0.0,  c,   s,   0.0],
                [0.0, -s,   c,   0.0],
                [0.0,  0.0, 0.0, 1.0],
            ],
        }
    }

    /// Full 4×4 matrix multiply: self × rhs.
    /// Counts as 1 mat_op.
    pub fn mul(&self, rhs: &Mat4) -> Mat4 {
        let a = &self.cols;
        let b = &rhs.cols;
        let mut out = [[0.0f32; 4]; 4];
        for col in 0..4 {
            for row in 0..4 {
                out[col][row] = a[0][row] * b[col][0]
                    + a[1][row] * b[col][1]
                    + a[2][row] * b[col][2]
                    + a[3][row] * b[col][3];
            }
        }
        Mat4 { cols: out }
    }

    /// Transform a Vec4 by this matrix.
    pub fn transform(&self, v: Vec4) -> Vec4 {
        let c = &self.cols;
        Vec4 {
            x: c[0][0]*v.x + c[1][0]*v.y + c[2][0]*v.z + c[3][0]*v.w,
            y: c[0][1]*v.x + c[1][1]*v.y + c[2][1]*v.z + c[3][1]*v.w,
            z: c[0][2]*v.x + c[1][2]*v.y + c[2][2]*v.z + c[3][2]*v.w,
            w: c[0][3]*v.x + c[1][3]*v.y + c[2][3]*v.z + c[3][3]*v.w,
        }
    }

    /// Perspective projection with [0,1] depth range (wgpu/WebGPU NDC).
    /// fov_y: vertical field of view in radians.
    /// aspect: width / height.
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let tan_half_fov = (fov_y / 2.0).tan();
        let range_inv = 1.0 / (near - far);  // negative

        Mat4 {
            cols: [
                [1.0 / (aspect * tan_half_fov), 0.0,                  0.0,                    0.0],
                [0.0,                           1.0 / tan_half_fov,   0.0,                    0.0],
                [0.0,                           0.0,                  far * range_inv,        -1.0],
                [0.0,                           0.0,                  far * near * range_inv,  0.0],
            ],
        }
    }

    pub fn translate(tx: f32, ty: f32, tz: f32) -> Mat4 {
        Mat4 {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [tx,  ty,  tz,  1.0],
            ],
        }
    }
}

impl Vec4 {
    pub fn xyz_dot(a: &Vec4, b: &Vec4) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z
    }

    pub fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
        if v < lo { lo } else if v > hi { hi } else { v }
    }
}

// ---------------------------------------------------------------------------
// Cube geometry
// Maps to: vertex data in VU data memory, loaded by VIF1 DMA from EE RAM.
// ---------------------------------------------------------------------------

struct CubeVertex {
    pos:    [f32; 3],
    normal: [f32; 3],
    color:  [f32; 3],
}

/// 36 vertices = 6 faces × 2 triangles × 3 vertices.
/// All triangles use CCW winding from outside the face (matches FrontFace::Ccw + back-face cull).
/// Face colors: +X=Red, -X=Cyan, +Y=Green, -Y=Magenta, +Z=Blue, -Z=Yellow
const CUBE_VERTS: [CubeVertex; 36] = [
    // +X face (Red, normal = [1, 0, 0])
    CubeVertex { pos: [ 1.0, -1.0, -1.0], normal: [1.0, 0.0, 0.0], color: [1.0, 0.1, 0.1] },
    CubeVertex { pos: [ 1.0,  1.0,  1.0], normal: [1.0, 0.0, 0.0], color: [1.0, 0.1, 0.1] },
    CubeVertex { pos: [ 1.0, -1.0,  1.0], normal: [1.0, 0.0, 0.0], color: [1.0, 0.1, 0.1] },
    CubeVertex { pos: [ 1.0, -1.0, -1.0], normal: [1.0, 0.0, 0.0], color: [1.0, 0.1, 0.1] },
    CubeVertex { pos: [ 1.0,  1.0, -1.0], normal: [1.0, 0.0, 0.0], color: [1.0, 0.1, 0.1] },
    CubeVertex { pos: [ 1.0,  1.0,  1.0], normal: [1.0, 0.0, 0.0], color: [1.0, 0.1, 0.1] },

    // -X face (Cyan, normal = [-1, 0, 0])
    CubeVertex { pos: [-1.0, -1.0,  1.0], normal: [-1.0, 0.0, 0.0], color: [0.1, 1.0, 1.0] },
    CubeVertex { pos: [-1.0,  1.0, -1.0], normal: [-1.0, 0.0, 0.0], color: [0.1, 1.0, 1.0] },
    CubeVertex { pos: [-1.0, -1.0, -1.0], normal: [-1.0, 0.0, 0.0], color: [0.1, 1.0, 1.0] },
    CubeVertex { pos: [-1.0, -1.0,  1.0], normal: [-1.0, 0.0, 0.0], color: [0.1, 1.0, 1.0] },
    CubeVertex { pos: [-1.0,  1.0,  1.0], normal: [-1.0, 0.0, 0.0], color: [0.1, 1.0, 1.0] },
    CubeVertex { pos: [-1.0,  1.0, -1.0], normal: [-1.0, 0.0, 0.0], color: [0.1, 1.0, 1.0] },

    // +Y face (Green, normal = [0, 1, 0])
    CubeVertex { pos: [-1.0,  1.0, -1.0], normal: [0.0, 1.0, 0.0], color: [0.1, 1.0, 0.1] },
    CubeVertex { pos: [ 1.0,  1.0,  1.0], normal: [0.0, 1.0, 0.0], color: [0.1, 1.0, 0.1] },
    CubeVertex { pos: [ 1.0,  1.0, -1.0], normal: [0.0, 1.0, 0.0], color: [0.1, 1.0, 0.1] },
    CubeVertex { pos: [-1.0,  1.0, -1.0], normal: [0.0, 1.0, 0.0], color: [0.1, 1.0, 0.1] },
    CubeVertex { pos: [-1.0,  1.0,  1.0], normal: [0.0, 1.0, 0.0], color: [0.1, 1.0, 0.1] },
    CubeVertex { pos: [ 1.0,  1.0,  1.0], normal: [0.0, 1.0, 0.0], color: [0.1, 1.0, 0.1] },

    // -Y face (Magenta, normal = [0, -1, 0])
    CubeVertex { pos: [-1.0, -1.0,  1.0], normal: [0.0, -1.0, 0.0], color: [1.0, 0.1, 1.0] },
    CubeVertex { pos: [ 1.0, -1.0, -1.0], normal: [0.0, -1.0, 0.0], color: [1.0, 0.1, 1.0] },
    CubeVertex { pos: [ 1.0, -1.0,  1.0], normal: [0.0, -1.0, 0.0], color: [1.0, 0.1, 1.0] },
    CubeVertex { pos: [-1.0, -1.0,  1.0], normal: [0.0, -1.0, 0.0], color: [1.0, 0.1, 1.0] },
    CubeVertex { pos: [-1.0, -1.0, -1.0], normal: [0.0, -1.0, 0.0], color: [1.0, 0.1, 1.0] },
    CubeVertex { pos: [ 1.0, -1.0, -1.0], normal: [0.0, -1.0, 0.0], color: [1.0, 0.1, 1.0] },

    // +Z face (Blue, normal = [0, 0, 1])
    CubeVertex { pos: [ 1.0, -1.0,  1.0], normal: [0.0, 0.0, 1.0], color: [0.1, 0.1, 1.0] },
    CubeVertex { pos: [-1.0,  1.0,  1.0], normal: [0.0, 0.0, 1.0], color: [0.1, 0.1, 1.0] },
    CubeVertex { pos: [-1.0, -1.0,  1.0], normal: [0.0, 0.0, 1.0], color: [0.1, 0.1, 1.0] },
    CubeVertex { pos: [ 1.0, -1.0,  1.0], normal: [0.0, 0.0, 1.0], color: [0.1, 0.1, 1.0] },
    CubeVertex { pos: [ 1.0,  1.0,  1.0], normal: [0.0, 0.0, 1.0], color: [0.1, 0.1, 1.0] },
    CubeVertex { pos: [-1.0,  1.0,  1.0], normal: [0.0, 0.0, 1.0], color: [0.1, 0.1, 1.0] },

    // -Z face (Yellow, normal = [0, 0, -1])
    CubeVertex { pos: [-1.0, -1.0, -1.0], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.1] },
    CubeVertex { pos: [ 1.0,  1.0, -1.0], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.1] },
    CubeVertex { pos: [ 1.0, -1.0, -1.0], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.1] },
    CubeVertex { pos: [-1.0, -1.0, -1.0], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.1] },
    CubeVertex { pos: [-1.0,  1.0, -1.0], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.1] },
    CubeVertex { pos: [ 1.0,  1.0, -1.0], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.1] },
];

// ---------------------------------------------------------------------------
// VU1 state
// ---------------------------------------------------------------------------

pub struct Vu1State {
    pub frame:    u64,
    pub mat_ops:  u64,
}

impl Vu1State {
    pub fn new() -> Self {
        Vu1State { frame: 0, mat_ops: 0 }
    }
}

// ---------------------------------------------------------------------------
// execute_micro_program — the VU1 micro-program in Rust
// Maps to: VU1 running its micro-program, ending with XGKICK → GS.
// Returns (display_list, mat_ops_this_frame).
// ---------------------------------------------------------------------------

pub fn execute_micro_program(vu: &mut Vu1State) -> (Vec<GsVertex>, u64) {
    let frame = vu.frame;
    vu.frame += 1;

    // Rotation angles: 1°/frame on Y, 0.5°/frame on X
    let angle_y = (frame as f32) * (PI / 180.0);
    let angle_x = (frame as f32) * (PI / 360.0);

    // Build model matrix: rot_x × rot_y  (mat_ops: 1 rotate_y, 1 rotate_x, 1 mul = 3)
    let rot_y  = Mat4::rotate_y(angle_y);
    let rot_x  = Mat4::rotate_x(angle_x);
    let model  = rot_x.mul(&rot_y);           // mat_op #1

    // View: camera at origin, cube 3 units in front
    let view = Mat4::translate(0.0, 0.0, -3.0);

    // Projection: PS2 standard 640×448 aspect, [0,1] depth range
    let proj = Mat4::perspective(PI / 3.0, 640.0 / 448.0, 0.1, 100.0);

    // Concatenate transforms (mat_ops: 2 more muls = total 5 this frame)
    let mv  = view.mul(&model);               // mat_op #2
    let mvp = proj.mul(&mv);                  // mat_op #3

    let mat_ops_this_frame: u64 = 5; // rotate_y + rotate_x + 3 muls
    vu.mat_ops += mat_ops_this_frame;

    // Light direction: normalized (1,1,1) → 0.5773 each
    let light_dir = Vec4 { x: 0.577, y: 0.577, z: 0.577, w: 0.0 };

    // Per-vertex transform + Gouraud lighting
    let mut display_list = Vec::with_capacity(CUBE_VERTS.len());

    for v in CUBE_VERTS.iter() {
        // Transform position to clip space (VU1 perspective divide)
        let pos_model = Vec4 { x: v.pos[0], y: v.pos[1], z: v.pos[2], w: 1.0 };
        let clip_pos  = mvp.transform(pos_model);

        // Transform normal by model matrix only (no perspective projection on normals)
        let norm_in   = Vec4 { x: v.normal[0], y: v.normal[1], z: v.normal[2], w: 0.0 };
        let world_norm = model.transform(norm_in);

        // Gouraud diffuse lighting
        let diffuse   = Vec4::clampf(Vec4::xyz_dot(&world_norm, &light_dir), 0.0, 1.0);
        let intensity = Vec4::clampf(diffuse + 0.2, 0.0, 1.0); // 0.2 ambient

        display_list.push(GsVertex {
            position: [clip_pos.x, clip_pos.y, clip_pos.z, clip_pos.w],
            color:    [v.color[0] * intensity, v.color[1] * intensity, v.color[2] * intensity, 1.0],
        });
    }

    (display_list, mat_ops_this_frame)
}
