use std::collections::BTreeSet;

use super::{mesh_chunk, BlockClass, Face, Mesh, Vertex, VoxelView};

const AIR: u32 = 0;
const STONE: u32 = 1;
const LEAF: u32 = 2;
const OPAQUE_B: u32 = 5;

struct TestView {
    height: u16,
    states: Vec<u32>,
    classes: Vec<BlockClass>,
    light: Option<Vec<(u8, u8)>>,
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
                BlockClass::Opaque,
            ],
            light: None,
        }
    }

    fn with_light(mut self, light: Vec<(u8, u8)>) -> Self {
        self.light = Some(light);
        self
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

    fn light_at(&self, x: u8, y: u16, z: u8) -> (u8, u8) {
        if let Some(light) = &self.light {
            return light.get(idx(x, y, z)).copied().unwrap_or((0, 0));
        }
        (
            ((x as usize + y as usize * 3) % 16) as u8,
            ((z as usize + y as usize * 7) % 16) as u8,
        )
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

fn flat_view() -> TestView {
    let mut states = vec![AIR; 16 * 16 * 16];
    for x in 0..16u8 {
        for z in 0..16u8 {
            states[idx(x, 0, z)] = STONE;
        }
    }
    TestView::new(16, states)
}

fn carved_view() -> TestView {
    let mut states = vec![STONE; 16 * 16 * 16];
    states[idx(0, 0, 0)] = AIR;
    states[idx(8, 0, 0)] = AIR;
    TestView::new(16, states)
}

fn split_plane_view() -> TestView {
    let mut states = vec![AIR; 16 * 16 * 16];
    for x in 0..16u8 {
        for z in 0..16u8 {
            states[idx(x, 0, z)] = if x < 8 { STONE } else { OPAQUE_B };
        }
    }
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

fn quad_geom(mesh: &Mesh, quad: &[u32]) -> (Face, [f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for &i in quad {
        let v = &mesh.vertices[i as usize];
        let p = [v.x, v.y, v.z];
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    let face = face_of(normal_of(mesh, quad)).expect("axis-aligned face");
    (face, min, max)
}

fn quad_plane_area(mesh: &Mesh, quad: &[u32]) -> (Face, i64, i64) {
    let (face, min, max) = quad_geom(mesh, quad);
    let axis = face.axis();
    let mut area = 1.0f32;
    for k in 0..3 {
        if k != axis {
            area *= (max[k] - min[k]).round();
        }
    }
    (face, (min[axis] + 0.5) as i64, area as i64)
}

fn assert_invariants(mesh: &Mesh) {
    let quads = mesh.indices.len() / 6;
    assert_eq!(mesh.vertex_count(), quads * 4);
    for &index in &mesh.indices {
        assert!((index as usize) < mesh.vertex_count());
    }
}

fn corner_vertex(mesh: &Mesh, face: Face, ref_point: [f32; 3], point: [f32; 3]) -> Vertex {
    for quad in mesh.indices.chunks(6) {
        let (quad_face, _min, _max) = quad_geom(mesh, quad);
        if quad_face != face {
            continue;
        }
        let has_ref = quad.iter().any(|&i| {
            let v = &mesh.vertices[i as usize];
            let p = [v.x, v.y, v.z];
            p.iter().zip(&ref_point).all(|(a, b)| (a - b).abs() < 1e-6)
        });
        if !has_ref {
            continue;
        }
        for &i in quad {
            let v = &mesh.vertices[i as usize];
            let p = [v.x, v.y, v.z];
            if p.iter().zip(&point).all(|(a, b)| (a - b).abs() < 1e-6) {
                return *v;
            }
        }
    }
    panic!("vertex not found");
}

fn ao_fixture(occl: [bool; 3]) -> TestView {
    let mut states = vec![AIR; 16 * 16 * 16];
    states[idx(8, 8, 8)] = STONE;
    if occl[0] {
        states[idx(9, 8, 8)] = OPAQUE_B;
    }
    if occl[1] {
        states[idx(8, 8, 9)] = OPAQUE_B;
    }
    if occl[2] {
        states[idx(9, 8, 9)] = OPAQUE_B;
    }
    TestView::new(16, states)
}

fn probe_ao(mesh: &Mesh) -> f32 {
    corner_vertex(mesh, Face::PosY, [8.0, 9.0, 8.0], [9.0, 9.0, 9.0]).ao
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
    assert_eq!(mesh.indices.len() / 6, 6);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.vertices.len(), 24);
    assert_eq!(mesh.vertex_count(), 24);
    assert_invariants(&mesh);

    let top_quads = mesh
        .indices
        .chunks(6)
        .filter(|quad| quad.iter().all(|&i| mesh.vertices[i as usize].y == 16.0))
        .count();
    assert_eq!(top_quads, 1);
}

#[test]
fn adjacent_pair_culls_shared_face() {
    let mesh = mesh_chunk(&pair_view()).opaque;
    assert_eq!(mesh.indices.len() / 6, 6);
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
    assert_eq!(mesh.indices.len() / 6, 6);
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

#[test]
fn flat_plane_merges_to_six_quads() {
    let mesh = mesh_chunk(&flat_view()).opaque;
    assert_eq!(mesh.indices.len() / 6, 6);
    assert_eq!(mesh.vertices.len(), 24);
    assert_invariants(&mesh);
    let expected: BTreeSet<(Face, i64)> = [
        (Face::PosY, 1),
        (Face::NegY, 0),
        (Face::PosX, 16),
        (Face::NegX, 0),
        (Face::PosZ, 16),
        (Face::NegZ, 0),
    ]
    .into_iter()
    .collect();
    let got: BTreeSet<(Face, i64)> = mesh
        .indices
        .chunks(6)
        .map(|quad| {
            let (face, min, _) = quad_geom(&mesh, quad);
            (face, (min[face.axis()] + 0.5) as i64)
        })
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn merged_quads_cover_every_exposed_cell() {
    let mesh = mesh_chunk(&carved_view()).opaque;
    let mut groups: Vec<((Face, i64), i64)> = Vec::new();
    let mut total = 0i64;
    for quad in mesh.indices.chunks(6) {
        let (face, plane, area) = quad_plane_area(&mesh, quad);
        total += area;
        match groups.iter_mut().find(|(key, _)| *key == (face, plane)) {
            Some((_, sum)) => *sum += area,
            None => groups.push(((face, plane), area)),
        }
    }
    assert_eq!(total, 1538);
    let expected: &[(Face, i64, i64)] = &[
        (Face::NegX, 0, 255),
        (Face::NegX, 9, 1),
        (Face::PosX, 8, 1),
        (Face::PosX, 16, 256),
        (Face::NegY, 0, 254),
        (Face::NegY, 1, 2),
        (Face::PosY, 16, 256),
        (Face::NegZ, 0, 254),
        (Face::NegZ, 1, 2),
        (Face::PosZ, 16, 256),
    ];
    for &(face, plane, cells) in expected {
        let got = groups
            .iter()
            .find(|(key, _)| *key == (face, plane))
            .map(|(_, sum)| *sum)
            .unwrap_or(0);
        assert_eq!(got, cells);
    }
}

#[test]
fn split_plane_stops_at_state_boundary() {
    let mesh = mesh_chunk(&split_plane_view()).opaque;
    let mut tops: Vec<([f32; 3], [f32; 3])> = Vec::new();
    for quad in mesh.indices.chunks(6) {
        let (face, min, max) = quad_geom(&mesh, quad);
        if face == Face::PosY && (min[1] - 1.0).abs() < 1e-4 {
            tops.push((min, max));
        }
    }
    assert_eq!(tops.len(), 2);
    for (min, max) in &tops {
        assert!((max[0] - min[0] - 8.0).abs() < 1e-4);
        assert!(min[0] <= 8.0 && max[0] >= 8.0);
    }
}

#[test]
fn quads_occupy_span_corners() {
    let mesh = mesh_chunk(&carved_view()).opaque;
    for (q, quad) in mesh.indices.chunks(6).enumerate() {
        let base = (q * 4) as u32;
        let want = [base, base + 1, base + 2, base, base + 2, base + 3];
        assert_eq!(&mesh.indices[q * 6..q * 6 + 6], &want[..]);
        let (_, min, max) = quad_geom(&mesh, quad);
        let mut corners: Vec<[f32; 3]> = Vec::new();
        for &i in quad {
            let v = &mesh.vertices[i as usize];
            let p = [v.x, v.y, v.z];
            for k in 0..3 {
                assert!((p[k] - min[k]).abs() < 1e-4 || (p[k] - max[k]).abs() < 1e-4);
            }
            if !corners.contains(&p) {
                corners.push(p);
            }
        }
        assert_eq!(corners.len(), 4);
    }
}

#[test]
fn uniform_light_bakes_full_bright() {
    let view = single_voxel_view().with_light(vec![(15u8, 15u8); 4096]);
    let mesh = mesh_chunk(&view).opaque;
    for v in &mesh.vertices {
        assert!((v.light - 1.0).abs() < 1e-6);
        assert!((v.ao - 0.0).abs() < 1e-6);
    }
}

#[test]
fn corner_light_reads_own_diagonal() {
    let mut light = vec![(0u8, 0u8); 4096];
    light[idx(9, 8, 9)] = (0, 15);
    let view = single_voxel_view().with_light(light);
    let mesh = mesh_chunk(&view).opaque;
    let v = corner_vertex(&mesh, Face::PosY, [8.0, 9.0, 8.0], [9.0, 9.0, 9.0]);
    assert!((v.light - 1.0).abs() < 1e-6);
    let v = corner_vertex(&mesh, Face::PosY, [8.0, 9.0, 8.0], [8.0, 9.0, 8.0]);
    assert!((v.light - 0.0).abs() < 1e-6);
    let v = corner_vertex(&mesh, Face::PosY, [8.0, 9.0, 8.0], [9.0, 9.0, 8.0]);
    assert!((v.light - 0.0).abs() < 1e-6);
    let v = corner_vertex(&mesh, Face::PosY, [8.0, 9.0, 8.0], [8.0, 9.0, 9.0]);
    assert!((v.light - 0.0).abs() < 1e-6);
}

#[test]
fn ao_matches_occlusion_matrix() {
    let cases: [([bool; 3], f32); 6] = [
        ([false, false, false], 0.0),
        ([false, false, true], 2.0 / 3.0),
        ([true, false, false], 2.0 / 3.0),
        ([true, false, true], 1.0 / 3.0),
        ([true, true, false], 1.0),
        ([true, true, true], 1.0),
    ];
    for (occl, want) in cases {
        let view = ao_fixture(occl);
        let mesh = mesh_chunk(&view).opaque;
        assert!((probe_ao(&mesh) - want).abs() < 1e-6, "occl {:?}", occl);
    }
}

#[test]
fn ao_both_sides_rule() {
    let view = ao_fixture([true, true, false]);
    let mesh = mesh_chunk(&view).opaque;
    assert!((probe_ao(&mesh) - 1.0).abs() < 1e-6);
}

#[test]
fn merged_top_face_out_of_bounds_light() {
    let view = solid_view()
        .with_light((0..4096u32).map(|i| (((i >> 4) & 15) as u8, 0u8)).collect());
    let mesh = mesh_chunk(&view).opaque;
    let quad = mesh
        .indices
        .chunks(6)
        .find(|quad| {
            let (face, _min, _max) = quad_geom(&mesh, quad);
            face == Face::PosY && quad.iter().all(|&i| mesh.vertices[i as usize].y == 16.0)
        })
        .expect("merged top face");
    for &i in quad {
        let v = &mesh.vertices[i as usize];
        assert!((v.light - 1.0).abs() < 1e-6);
        assert!((v.ao - 0.0).abs() < 1e-6);
    }
}

#[test]
fn out_of_bounds_corner_light() {
    let mut states = vec![AIR; 16 * 16 * 16];
    states[idx(15, 15, 15)] = STONE;
    let view = TestView::new(16, states).with_light(vec![(0u8, 0u8); 4096]);
    let mesh = mesh_chunk(&view).opaque;
    let v = corner_vertex(&mesh, Face::PosX, [16.0, 15.0, 15.0], [16.0, 16.0, 16.0]);
    assert!((v.light - 1.0).abs() < 1e-6);
    assert!((v.ao - 0.0).abs() < 1e-6);
    let v = corner_vertex(&mesh, Face::PosX, [16.0, 15.0, 15.0], [16.0, 15.0, 15.0]);
    assert!((v.light - 0.0).abs() < 1e-6);
    assert!((v.ao - 0.0).abs() < 1e-6);
    let v = corner_vertex(&mesh, Face::PosY, [15.0, 16.0, 15.0], [16.0, 16.0, 16.0]);
    assert!((v.light - 1.0).abs() < 1e-6);
    assert!((v.ao - 0.0).abs() < 1e-6);
    let v = corner_vertex(&mesh, Face::PosZ, [15.0, 15.0, 16.0], [16.0, 16.0, 16.0]);
    assert!((v.light - 1.0).abs() < 1e-6);
    assert!((v.ao - 0.0).abs() < 1e-6);
}

#[test]
fn corner_light_matches_span_corner_voxel() {
    let view = carved_view();
    let mesh = mesh_chunk(&view).opaque;
    let (w, h, d) = view.dims();
    let dims = [w as i32, h as i32, d as i32];
    for quad in mesh.indices.chunks(6) {
        let (face, _min, max) = quad_geom(&mesh, quad);
        let axis = face.axis();
        let (b, c) = face.cross();
        for &i in quad {
            let v = &mesh.vertices[i as usize];
            let p = [v.x, v.y, v.z];
            let mut solid = [0i32; 3];
            for k in 0..3 {
                solid[k] = if k == axis {
                    if face.sign() > 0 {
                        p[k] as i32 - 1
                    } else {
                        p[k] as i32
                    }
                } else if (p[k] - max[k]).abs() < 1e-4 {
                    p[k] as i32 - 1
                } else {
                    p[k] as i32
                };
            }
            let db = if (p[b] - max[b]).abs() < 1e-4 { 1 } else { -1 };
            let dc = if (p[c] - max[c]).abs() < 1e-4 { 1 } else { -1 };
            let mut s1 = solid;
            s1[b] += db;
            let mut s2 = solid;
            s2[c] += dc;
            let mut sc = solid;
            sc[b] += db;
            sc[c] += dc;
            let cells = [s1, s2, sc];
            let mut sky = 0i32;
            let mut block = 0i32;
            let mut occl = [false; 3];
            for k in 0..3 {
                let cell = cells[k];
                let oob = (0..3).any(|k| cell[k] < 0 || cell[k] >= dims[k]);
                if oob {
                    sky = 15;
                    block = 15;
                } else {
                    let l = view.light_at(cell[0] as u8, cell[1] as u16, cell[2] as u8);
                    sky = sky.max(l.0 as i32);
                    block = block.max(l.1 as i32);
                    occl[k] =
                        view.class_of(view.state_at(cell[0] as u8, cell[1] as u16, cell[2] as u8))
                            == BlockClass::Opaque;
                }
            }
            let want_light = sky.max(block) as f32 / 15.0;
            assert!((v.light - want_light).abs() < 1e-6);
            let n = occl.iter().filter(|x| **x).count() as i32;
            let mc_ao = if n == 0 {
                0
            } else if occl[0] && occl[1] {
                3
            } else {
                3 - n
            };
            assert!((v.ao - mc_ao as f32 / 3.0).abs() < 1e-6);
        }
    }
}
