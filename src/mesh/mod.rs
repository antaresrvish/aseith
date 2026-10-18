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

const BLOCK_TEXEL: f32 = 16.0 / 256.0;

pub fn mesh_chunk(view: &dyn VoxelView) -> ChunkMesh {
    let (w, h, d) = view.dims();
    let mut opaque = Mesh::default();
    emit_opaque(view, w, h, d, &mut opaque);
    let mut cutout = Mesh::default();
    emit_pass(view, w, h, d, BlockClass::Cutout, &mut cutout);
    let mut translucent = Mesh::default();
    emit_pass(view, w, h, d, BlockClass::Translucent, &mut translucent);
    let mut liquid = Mesh::default();
    emit_pass(view, w, h, d, BlockClass::Liquid, &mut liquid);
    ChunkMesh {
        opaque,
        cutout,
        translucent,
        liquid,
    }
}

fn emit_opaque(view: &dyn VoxelView, w: u8, h: u16, d: u8, mesh: &mut Mesh) {
    let dims = [w as usize, h as usize, d as usize];
    for face in [
        Face::PosX,
        Face::NegX,
        Face::PosY,
        Face::NegY,
        Face::PosZ,
        Face::NegZ,
    ] {
        emit_face(view, dims, face, mesh);
    }
}

fn emit_face(view: &dyn VoxelView, dims: [usize; 3], face: Face, mesh: &mut Mesh) {
    let a = face.axis();
    let (b, c) = face.cross();
    let dim_a = dims[a];
    let dim_b = dims[b];
    let dim_c = dims[c];
    let dims16 = [dims[0] as i16, dims[1] as i16, dims[2] as i16];
    let mut grid = vec![None; dim_b * dim_c];
    for p in 0..dim_a {
        for i in 0..dim_b {
            for j in 0..dim_c {
                grid[i * dim_c + j] = cell_state(view, dims, face, p, i, j);
            }
        }
        for (i, j, h, w, state) in greedy(&mut grid, dim_b, dim_c) {
            emit_quad(view, face, dims16, p, i, j, h, w, state, mesh);
        }
    }
}

fn cell_state(view: &dyn VoxelView, dims: [usize; 3], face: Face, p: usize, i: usize, j: usize) -> Option<u32> {
    let a = face.axis();
    let s = face.sign();
    let (b, c) = face.cross();
    let mut coord = [0usize; 3];
    coord[a] = p;
    coord[b] = i;
    coord[c] = j;
    let state = state_at(view, &coord);
    if view.class_of(state) != BlockClass::Opaque {
        return None;
    }
    let neighbor = if s > 0 {
        if p + 1 >= dims[a] {
            return Some(state);
        }
        p + 1
    } else {
        if p == 0 {
            return Some(state);
        }
        p - 1
    };
    let mut ncoord = coord;
    ncoord[a] = neighbor;
    if view.class_of(state_at(view, &ncoord)) == BlockClass::Opaque {
        return None;
    }
    Some(state)
}

fn state_at(view: &dyn VoxelView, coord: &[usize; 3]) -> u32 {
    view.state_at(coord[0] as u8, coord[1] as u16, coord[2] as u8)
}

fn emit_pass(view: &dyn VoxelView, w: u8, h: u16, d: u8, class: BlockClass, mesh: &mut Mesh) {
    let dims = [w as usize, h as usize, d as usize];
    let dims16 = [dims[0] as i16, dims[1] as i16, dims[2] as i16];
    let mut quads: Vec<(f32, [Vertex; 4])> = Vec::new();
    for face in [
        Face::PosX,
        Face::NegX,
        Face::PosY,
        Face::NegY,
        Face::PosZ,
        Face::NegZ,
    ] {
        let a = face.axis();
        let (b, c) = face.cross();
        for p in 0..dims[a] {
            for i in 0..dims[b] {
                for j in 0..dims[c] {
                    let mut coord = [0usize; 3];
                    coord[a] = p;
                    coord[b] = i;
                    coord[c] = j;
                    let state = state_at(view, &coord);
                    if view.class_of(state) != class {
                        continue;
                    }
                    if cell_culled(view, dims, face, &coord, class, state) {
                        continue;
                    }
                    if class == BlockClass::Cutout {
                        emit_double_quad(view, face, dims16, p, i, j, 1, 1, state, mesh);
                    } else {
                        let verts = corner_vertices(view, face, dims16, p, i, j, 1, 1, state);
                        quads.push((centroid_sum(&verts), verts));
                    }
                }
            }
        }
    }
    quads.sort_by(|x, y| y.0.total_cmp(&x.0));
    for (_, verts) in quads {
        let base = mesh.vertices.len() as u32;
        mesh.vertices.extend_from_slice(&verts);
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn cell_culled(
    view: &dyn VoxelView,
    dims: [usize; 3],
    face: Face,
    coord: &[usize; 3],
    class: BlockClass,
    state: u32,
) -> bool {
    let a = face.axis();
    let p = coord[a];
    let s = face.sign();
    let neighbor = if s > 0 {
        if p + 1 >= dims[a] {
            return false;
        }
        p + 1
    } else {
        if p == 0 {
            return false;
        }
        p - 1
    };
    let mut ncoord = *coord;
    ncoord[a] = neighbor;
    let nstate = state_at(view, &ncoord);
    let nclass = view.class_of(nstate);
    match class {
        BlockClass::Cutout => nclass == BlockClass::Opaque,
        BlockClass::Translucent => {
            nclass == BlockClass::Opaque
                || (nclass == BlockClass::Translucent && nstate == state)
        }
        BlockClass::Liquid => nclass == BlockClass::Liquid,
        _ => false,
    }
}

fn centroid_sum(verts: &[Vertex; 4]) -> f32 {
    verts.iter().map(|v| v.x + v.y + v.z).sum::<f32>()
}

fn in_bounds(dims: [i16; 3], cell: [i16; 3]) -> bool {
    (0..dims[0]).contains(&cell[0]) && (0..dims[1]).contains(&cell[1]) && (0..dims[2]).contains(&cell[2])
}

fn is_solid(view: &dyn VoxelView, dims: [i16; 3], cell: [i16; 3]) -> bool {
    in_bounds(dims, cell) && view.class_of(view.state_at(cell[0] as u8, cell[1] as u16, cell[2] as u8)) == BlockClass::Opaque
}

fn sample_cells(face: Face, solid: [i16; 3], corner: [i16; 3]) -> [[i16; 3]; 3] {
    let (b, c) = face.cross();
    let db: i16 = if corner[b] > 0 { 1 } else { -1 };
    let dc: i16 = if corner[c] > 0 { 1 } else { -1 };
    let mut s1 = solid;
    s1[b] += db;
    let mut s2 = solid;
    s2[c] += dc;
    let mut sc = solid;
    sc[b] += db;
    sc[c] += dc;
    [s1, s2, sc]
}

fn corner_light_ao(view: &dyn VoxelView, dims: [i16; 3], face: Face, solid: [i16; 3], corner: [i16; 3]) -> (f32, f32) {
    let cells = sample_cells(face, solid, corner);
    let mut sky = 0u8;
    let mut block = 0u8;
    let mut occl = [false; 3];
    for k in 0..3 {
        let cell = cells[k];
        if in_bounds(dims, cell) {
            let l = view.light_at(cell[0] as u8, cell[1] as u16, cell[2] as u8);
            sky = sky.max(l.0);
            block = block.max(l.1);
            occl[k] = is_solid(view, dims, cell);
        } else {
            sky = 15;
            block = 15;
        }
    }
    let light = sky.max(block) as f32 / 15.0;
    let n = occl.iter().filter(|x| **x).count();
    let mc_ao = if n == 0 {
        0
    } else if occl[0] && occl[1] {
        3
    } else {
        3 - (occl[0] as i16 + occl[1] as i16 + occl[2] as i16)
    };
    (light, mc_ao as f32 / 3.0)
}

fn greedy(grid: &mut [Option<u32>], rows: usize, cols: usize) -> Vec<(usize, usize, usize, usize, u32)> {
    let mut quads = Vec::new();
    for i in 0..rows {
        for j in 0..cols {
            let state = match grid[i * cols + j] {
                Some(state) => state,
                None => continue,
            };
            let mut w = 0;
            while j + w < cols && grid[i * cols + j + w] == Some(state) {
                w += 1;
            }
            let mut h = 0;
            'height: while i + h < rows {
                for k in 0..w {
                    if grid[(i + h) * cols + j + k] != Some(state) {
                        break 'height;
                    }
                }
                h += 1;
            }
            quads.push((i, j, h, w, state));
            for di in 0..h {
                for dk in 0..w {
                    grid[(i + di) * cols + j + dk] = None;
                }
            }
        }
    }
    quads
}

fn emit_quad(
    view: &dyn VoxelView,
    face: Face,
    dims: [i16; 3],
    p: usize,
    i: usize,
    j: usize,
    h: usize,
    w: usize,
    state: u32,
    mesh: &mut Mesh,
) {
    let verts = corner_vertices(view, face, dims, p, i, j, h, w, state);
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend_from_slice(&verts);
    mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn emit_double_quad(
    view: &dyn VoxelView,
    face: Face,
    dims: [i16; 3],
    p: usize,
    i: usize,
    j: usize,
    h: usize,
    w: usize,
    state: u32,
    mesh: &mut Mesh,
) {
    let verts = corner_vertices(view, face, dims, p, i, j, h, w, state);
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend_from_slice(&verts);
    mesh.vertices.extend_from_slice(&verts);
    mesh.indices.extend_from_slice(&[
        base,
        base + 1,
        base + 2,
        base,
        base + 2,
        base + 3,
        base + 7,
        base + 6,
        base + 5,
        base + 7,
        base + 4,
        base + 5,
    ]);
}

fn corner_vertices(
    view: &dyn VoxelView,
    face: Face,
    dims: [i16; 3],
    p: usize,
    i: usize,
    j: usize,
    h: usize,
    w: usize,
    state: u32,
) -> [Vertex; 4] {
    let a = face.axis();
    let s = face.sign();
    let (b, c) = face.cross();
    let plane = p + if s > 0 { 1 } else { 0 };
    let (u0, v0) = view.uv_of(state, face);
    let span_u = h as f32 * BLOCK_TEXEL;
    let span_v = w as f32 * BLOCK_TEXEL;
    let corners = face.corners();
    let mut verts = [Vertex::default(); 4];
    for k in 0..4 {
        let off = corners[k];
        let (coord_b, voxel_b) = if off[b] > 0 {
            (i + h, i + h - 1)
        } else {
            (i, i)
        };
        let (coord_c, voxel_c) = if off[c] > 0 {
            (j + w, j + w - 1)
        } else {
            (j, j)
        };
        let mut solid = [0i16; 3];
        solid[a] = p as i16;
        solid[b] = voxel_b as i16;
        solid[c] = voxel_c as i16;
        let (light, ao) = corner_light_ao(view, dims, face, solid, off);
        let mut pos = [0.0f32; 3];
        pos[a] = plane as f32;
        pos[b] = coord_b as f32;
        pos[c] = coord_c as f32;
        let u = u0 + if off[b] > 0 { span_u } else { 0.0 };
        let v = v0 + if off[c] > 0 { span_v } else { 0.0 };
        verts[k] = Vertex {
            x: pos[0],
            y: pos[1],
            z: pos[2],
            u,
            v,
            light,
            ao,
        };
    }
    verts
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

    fn axis(&self) -> usize {
        let (ox, oy, _) = self.offset();
        if ox != 0 {
            0
        } else if oy != 0 {
            1
        } else {
            2
        }
    }

    fn sign(&self) -> i8 {
        let (ox, oy, oz) = self.offset();
        let v = if ox != 0 {
            ox
        } else if oy != 0 {
            oy
        } else {
            oz
        };
        if v > 0 {
            1
        } else {
            -1
        }
    }

    fn cross(&self) -> (usize, usize) {
        match self {
            Face::PosX | Face::NegX => (1, 2),
            Face::PosY | Face::NegY => (0, 2),
            Face::PosZ | Face::NegZ => (0, 1),
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
