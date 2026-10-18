use std::mem::size_of;

use crate::gpu::{ChunkBuffers, PassBuffers};
use crate::mesh::{mesh_chunk, BlockClass, ChunkMesh, Face, Mesh, Vertex, VoxelView};

const AIR: u32 = 0;
const STONE: u32 = 1;
const LEAF: u32 = 2;
const GLASS: u32 = 3;
const WATER: u32 = 4;

struct ClassView {
    states: Vec<u32>,
}

impl VoxelView for ClassView {
    fn dims(&self) -> (u8, u16, u8) {
        (16, 16, 16)
    }

    fn state_at(&self, x: u8, y: u16, z: u8) -> u32 {
        self.states.get(idx(x, y, z)).copied().unwrap_or(AIR)
    }

    fn class_of(&self, state: u32) -> BlockClass {
        match state {
            STONE => BlockClass::Opaque,
            LEAF => BlockClass::Cutout,
            GLASS => BlockClass::Translucent,
            WATER => BlockClass::Liquid,
            _ => BlockClass::Air,
        }
    }

    fn light_at(&self, _x: u8, _y: u16, _z: u8) -> (u8, u8) {
        (15, 15)
    }

    fn uv_of(&self, _state: u32, _face: Face) -> (f32, f32) {
        (0.0, 0.0)
    }
}

fn idx(x: u8, y: u16, z: u8) -> usize {
    (y as usize) * 256 + (x as usize) * 16 + z as usize
}

fn multi_class_view() -> ClassView {
    let mut states = vec![AIR; 16 * 16 * 16];
    states[idx(4, 8, 4)] = STONE;
    states[idx(4, 8, 12)] = LEAF;
    states[idx(12, 8, 4)] = GLASS;
    states[idx(12, 8, 12)] = WATER;
    ClassView { states }
}

fn passes<'a>(mesh: &'a ChunkMesh, buffers: &'a ChunkBuffers) -> [(&'a Mesh, &'a PassBuffers); 4] {
    [
        (&mesh.opaque, &buffers.opaque),
        (&mesh.cutout, &buffers.cutout),
        (&mesh.translucent, &buffers.translucent),
        (&mesh.liquid, &buffers.liquid),
    ]
}

fn unpack_vertex(chunk: &[u8]) -> Vertex {
    let read = |offset: usize| f32::from_le_bytes(chunk[offset..offset + 4].try_into().unwrap());
    Vertex {
        x: read(0),
        y: read(4),
        z: read(8),
        u: read(12),
        v: read(16),
        light: read(20),
        ao: read(24),
    }
}

fn unpack_indices(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(size_of::<u32>())
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn assert_pass_sizes(pass: &PassBuffers, vertices: usize, indices: usize) {
    assert_eq!(pass.vertex_count, vertices);
    assert_eq!(pass.vertices.len(), vertices * size_of::<Vertex>());
    assert_eq!(pass.indices.len(), indices * size_of::<u32>());
}

#[test]
fn empty_chunk_mesh_packs_empty_buffers() {
    let mesh = ChunkMesh::default();
    let buffers = ChunkBuffers::from_chunk_mesh(&mesh);
    for (pass_mesh, pass) in passes(&mesh, &buffers) {
        assert!(pass.vertices.is_empty());
        assert!(pass.indices.is_empty());
        assert_eq!(pass.vertex_count, pass_mesh.vertex_count());
        assert_eq!(pass.vertex_count, 0);
    }
}

#[test]
fn packed_bytes_match_mesh_sizes() {
    let mesh = mesh_chunk(&multi_class_view());
    let buffers = ChunkBuffers::from_chunk_mesh(&mesh);
    assert_eq!(size_of::<Vertex>(), 28);
    for (pass_mesh, pass) in passes(&mesh, &buffers) {
        assert_eq!(pass.vertices.len(), pass_mesh.vertices.len() * size_of::<Vertex>());
        assert_eq!(pass.indices.len(), pass_mesh.indices.len() * size_of::<u32>());
        assert_eq!(pass.vertex_count, pass_mesh.vertex_count());
    }
}

#[test]
fn packed_bytes_roundtrip_bit_exact() {
    let mesh = mesh_chunk(&multi_class_view());
    let buffers = ChunkBuffers::from_chunk_mesh(&mesh);
    for (pass_mesh, pass) in passes(&mesh, &buffers) {
        assert_eq!(pass.vertices.len(), pass.vertex_count * size_of::<Vertex>());
        let vertices = pass
            .vertices
            .chunks_exact(size_of::<Vertex>())
            .map(unpack_vertex)
            .collect::<Vec<Vertex>>();
        assert_eq!(vertices, pass_mesh.vertices);
        assert_eq!(unpack_indices(&pass.indices), pass_mesh.indices);
    }
}

#[test]
fn one_voxel_per_class_end_to_end() {
    let mesh = mesh_chunk(&multi_class_view());
    assert_eq!(mesh.opaque.indices.len() / 6, 6);
    assert_eq!(mesh.cutout.indices.len() / 6, 12);
    assert_eq!(mesh.translucent.indices.len() / 6, 6);
    assert_eq!(mesh.liquid.indices.len() / 6, 6);

    let buffers = ChunkBuffers::from_chunk_mesh(&mesh);
    assert_pass_sizes(&buffers.opaque, 24, 36);
    assert_pass_sizes(&buffers.cutout, 48, 72);
    assert_pass_sizes(&buffers.translucent, 24, 36);
    assert_pass_sizes(&buffers.liquid, 24, 36);
}
