use std::mem::size_of;

use crate::mesh::{ChunkMesh, Mesh, Vertex};

#[derive(Default)]
pub struct PassBuffers {
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub vertex_count: usize,
}

#[derive(Default)]
pub struct ChunkBuffers {
    pub opaque: PassBuffers,
    pub cutout: PassBuffers,
    pub translucent: PassBuffers,
    pub liquid: PassBuffers,
}

impl PassBuffers {
    pub fn from_mesh(mesh: &Mesh) -> Self {
        let vertex_stride = size_of::<Vertex>();
        let mut vertices = Vec::with_capacity(mesh.vertices.len() * vertex_stride);
        for vertex in &mesh.vertices {
            vertices.extend_from_slice(&vertex.x.to_le_bytes());
            vertices.extend_from_slice(&vertex.y.to_le_bytes());
            vertices.extend_from_slice(&vertex.z.to_le_bytes());
            vertices.extend_from_slice(&vertex.u.to_le_bytes());
            vertices.extend_from_slice(&vertex.v.to_le_bytes());
            vertices.extend_from_slice(&vertex.light.to_le_bytes());
            vertices.extend_from_slice(&vertex.ao.to_le_bytes());
        }
        let mut indices = Vec::with_capacity(mesh.indices.len() * size_of::<u32>());
        for index in &mesh.indices {
            indices.extend_from_slice(&index.to_le_bytes());
        }
        PassBuffers {
            vertices,
            indices,
            vertex_count: mesh.vertex_count(),
        }
    }
}

impl ChunkBuffers {
    pub fn from_chunk_mesh(mesh: &ChunkMesh) -> Self {
        ChunkBuffers {
            opaque: PassBuffers::from_mesh(&mesh.opaque),
            cutout: PassBuffers::from_mesh(&mesh.cutout),
            translucent: PassBuffers::from_mesh(&mesh.translucent),
            liquid: PassBuffers::from_mesh(&mesh.liquid),
        }
    }
}

#[cfg(test)]
mod tests;
