use std::collections::BTreeSet;

use super::{mesh_chunk, BlockClass, Face, Mesh, VoxelView};

const AIR: u32 = 0;
const STONE: u32 = 1;
const LEAF: u32 = 2;

struct TestView {
    height: u16,
    states: Vec<u32>,
    classes: Vec<BlockClass>,
}

impl TestView {
    fn new(height: u16, states: Vec<u32>) -> Self {
        Self {
            height,
            states,
            classes: vec![
                BlockClass::Air,
                BlockClass::Opaque,
                BlockClass::Cutout,
                BlockClass::Translucent,
                BlockClass::Liquid,
            ],
        }
    }
}

impl VoxelView for TestView {
    fn dims(&self) -> (u8, u16, u8) {
        (16, self.height, 16)
    }

    fn state_at(&self, x: u8, y: u16, z: u8) -> u32 {
        let index = (y as usize) * 256 + (x as usize) * 16 + z as usize;
        self.states.get(index).copied().unwrap_or(AIR)
    }

    fn class_of(&self, state: u32) -> BlockClass {
        self.classes.get(state as usize).copied().unwrap_or(BlockClass::Air)
    }

    fn light_at(&self, _x: u8, _y: u16, _z: u8) -> (u8, u8) {
        (0, 0)
    }

    fn uv_of(&self, _state: u32, _face: Face) -> (f32, f32) {
        (0.0, 0.0)
    }
}

fn idx(x: u8, y: u16, z: u8) -> usize {
    (y as usize) * 256 + (x as usize) * 16 + z as usize
}

fn single_voxel_view() -> TestView {
    let mut states = vec![AIR; 16 * 16 * 16];
    states[idx(8, 8, 8)] = STONE;
    TestView::new(16, states)
}

fn solid_view() -> TestView {
    TestView::new(16, vec![STONE; 16 * 16 * 16])
}

fn pair_view() -> TestView {
    let mut states = vec![AIR; 16 * 16 * 16];
    states[idx(8, 8, 8)] = STONE;
    states[idx(9, 8, 8)] = STONE;
    TestView::new(16, states)
}

fn cutout_above_view() -> TestView {
    let mut states = vec![AIR; 16 * 16 * 16];
    states[idx(8, 8, 8)] = STONE;
    states[idx(8, 9, 8)] = LEAF;
    TestView::new(16, states)
}

fn opaque_above_view() -> TestView {
    let mut states = vec![AIR; 16 * 16 * 16];
    states[idx(8, 8, 8)] = STONE;
    states[idx(8, 9, 8)] = STONE;
    TestView::new(16, states)
}

fn normal_of(mesh: &Mesh, quad: &[u32]) -> (f32, f32, f32) {
    let a = mesh.vertices[quad[0] as usize];
    let b = mesh.vertices[quad[1] as usize];
    let c = mesh.vertices[quad[2] as usize];
    let (ux, uy, uz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let (vx, vy, vz) = (c.x - a.x, c.y - a.y, c.z - a.z);
    (
        uy * vz - uz * vy,
        uz * vx - ux * vz,
        ux * vy - uy * vx,
    )
}

fn face_of(normal: (f32, f32, f32)) -> Option<Face> {
    let (nx, ny, nz) = normal;
    if nx.abs() > 0.5 {
        return Some(if nx > 0.0 { Face::PosX } else { Face::NegX });
    }
    if ny.abs() > 0.5 {
        return Some(if ny > 0.0 { Face::PosY } else { Face::NegY });
    }
    if nz.abs() > 0.5 {
        return Some(if nz > 0.0 { Face::PosZ } else { Face::NegZ });
    }
    None
}

fn assert_invariants(mesh: &Mesh) {
    let quads = mesh.indices.len() / 6;
    assert_eq!(mesh.vertex_count(), quads * 4);
    for &index in &mesh.indices {
        assert!((index as usize) < mesh.vertex_count());
    }
}

#[test]
fn single_voxel_emits_six_quads() {
    let mesh = mesh_chunk(&single_voxel_view()).opaque;
    assert_eq!(mesh.vertices.len(), 24);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.vertex_count(), 24);

    let faces: BTreeSet<Face> = mesh
        .indices
        .chunks(6)
        .map(|quad| face_of(normal_of(&mesh, quad)).expect("axis-aligned face"))
        .collect();
    let all: BTreeSet<Face> = [
        Face::PosX,
        Face::NegX,
        Face::PosY,
        Face::NegY,
        Face::PosZ,
        Face::NegZ,
    ]
    .into_iter()
    .collect();
    assert_eq!(faces, all);
}

#[test]
fn solid_view_emits_shell_only() {
    let mesh = mesh_chunk(&solid_view()).opaque;
    assert_eq!(mesh.indices.len() / 6, 1536);
    assert_eq!(mesh.vertices.len(), 1536 * 4);
    assert_invariants(&mesh);
}

#[test]
fn adjacent_pair_culls_shared_face() {
    let mesh = mesh_chunk(&pair_view()).opaque;
    assert_eq!(mesh.indices.len() / 6, 10);
    assert_invariants(&mesh);

    let shared_plane_quads = mesh
        .indices
        .chunks(6)
        .filter(|quad| quad.iter().all(|&i| mesh.vertices[i as usize].x == 9.0))
        .count();
    assert_eq!(shared_plane_quads, 0);
}

#[test]
fn cutout_neighbor_does_not_occlude() {
    let mesh = mesh_chunk(&cutout_above_view()).opaque;
    assert_eq!(mesh.indices.len() / 6, 6);
    assert_invariants(&mesh);

    let top_emitted = mesh.indices.chunks(6).any(|quad| {
        face_of(normal_of(&mesh, quad)) == Some(Face::PosY)
            && quad.iter().all(|&i| mesh.vertices[i as usize].y == 9.0)
    });
    assert!(top_emitted);
}

#[test]
fn opaque_neighbor_culls_top_face() {
    let mesh = mesh_chunk(&opaque_above_view()).opaque;
    assert_eq!(mesh.indices.len() / 6, 10);
    assert_invariants(&mesh);

    let top_emitted = mesh
        .indices
        .chunks(6)
        .any(|quad| quad.iter().all(|&i| mesh.vertices[i as usize].y == 9.0));
    assert!(!top_emitted);
}

#[test]
fn winding_is_outward() {
    let mesh = mesh_chunk(&single_voxel_view()).opaque;
    let mut seen = [false; 6];
    for quad in mesh.indices.chunks(6) {
        let normal = normal_of(&mesh, quad);
        let face = face_of(normal).expect("axis-aligned face");
        let (ox, oy, oz) = face.offset();
        assert_eq!(normal, (ox as f32, oy as f32, oz as f32));
        seen[face as usize] = true;
    }
    assert!(seen.iter().all(|s| *s));
}

#[test]
fn other_layers_stay_empty() {
    let chunk = mesh_chunk(&solid_view());
    assert!(chunk.cutout.vertices.is_empty());
    assert!(chunk.cutout.indices.is_empty());
    assert!(chunk.translucent.vertices.is_empty());
    assert!(chunk.translucent.indices.is_empty());
    assert!(chunk.liquid.vertices.is_empty());
    assert!(chunk.liquid.indices.is_empty());
}
