#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockClass {
    Air,
    Opaque,
    Cutout,
    Translucent,
    Liquid,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub light: f32,
    pub ao: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Face {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

#[derive(Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.indices.iter().copied().max().map(|max| max as usize + 1).unwrap_or(0)
    }
}

#[derive(Debug, Default)]
pub struct ChunkMesh {
    pub opaque: Mesh,
    pub cutout: Mesh,
    pub translucent: Mesh,
    pub liquid: Mesh,
}

pub trait VoxelView {
    fn dims(&self) -> (u8, u16, u8);
    fn state_at(&self, x: u8, y: u16, z: u8) -> u32;
    fn class_of(&self, state: u32) -> BlockClass;
    fn light_at(&self, x: u8, y: u16, z: u8) -> (u8, u8);
    fn uv_of(&self, state: u32, face: Face) -> (f32, f32);
}

pub fn mesh_chunk(view: &dyn VoxelView) -> ChunkMesh {
    let (w, h, d) = view.dims();
    let voxels = w as usize * h as usize * d as usize;
    let mut opaque = Mesh {
        vertices: Vec::with_capacity(voxels * 4),
        indices: Vec::with_capacity(voxels * 6),
    };

    for y in 0..h {
        for z in 0..d {
            for x in 0..w {
                let state = view.state_at(x, y, z);
                if view.class_of(state) != BlockClass::Opaque {
                    continue;
                }
                for face in [
                    Face::PosX,
                    Face::NegX,
                    Face::PosY,
                    Face::NegY,
                    Face::PosZ,
                    Face::NegZ,
                ] {
                    if neighbor_is_opaque(view, w, h, d, x, y, z, face) {
                        continue;
                    }
                    let (u, v) = view.uv_of(state, face);
                    let (sky, block) = view.light_at(x, y, z);
                    let light = sky.max(block) as f32 / 15.0;
                    let base = opaque.vertices.len() as u32;
                    for corner in face.corners() {
                        opaque.vertices.push(Vertex {
                            x: x as f32 + corner[0] as f32,
                            y: y as f32 + corner[1] as f32,
                            z: z as f32 + corner[2] as f32,
                            u,
                            v,
                            light,
                            ao: 0.0,
                        });
                    }
                    opaque.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
        }
    }

    ChunkMesh {
        opaque,
        cutout: Mesh::default(),
        translucent: Mesh::default(),
        liquid: Mesh::default(),
    }
}

fn neighbor_is_opaque(view: &dyn VoxelView, w: u8, h: u16, d: u8, x: u8, y: u16, z: u8, face: Face) -> bool {
    let (ox, oy, oz) = face.offset();
    let nx = x as i16 + ox;
    let ny = y as i16 + oy;
    let nz = z as i16 + oz;
    if nx < 0 || nx >= w as i16 || ny < 0 || ny >= h as i16 || nz < 0 || nz >= d as i16 {
        return false;
    }
    let neighbor = view.state_at(nx as u8, ny as u16, nz as u8);
    view.class_of(neighbor) == BlockClass::Opaque
}

impl Face {
    fn offset(&self) -> (i16, i16, i16) {
        match self {
            Face::PosX => (1, 0, 0),
            Face::NegX => (-1, 0, 0),
            Face::PosY => (0, 1, 0),
            Face::NegY => (0, -1, 0),
            Face::PosZ => (0, 0, 1),
            Face::NegZ => (0, 0, -1),
        }
    }

    fn corners(&self) -> [[i16; 3]; 4] {
        match self {
            Face::PosX => [[1, 0, 1], [1, 0, 0], [1, 1, 0], [1, 1, 1]],
            Face::NegX => [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]],
            Face::PosY => [[0, 1, 1], [1, 1, 1], [1, 1, 0], [0, 1, 0]],
            Face::NegY => [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]],
            Face::PosZ => [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]],
            Face::NegZ => [[1, 0, 0], [0, 0, 0], [0, 1, 0], [1, 1, 0]],
        }
    }
}

#[cfg(test)]
mod tests;
