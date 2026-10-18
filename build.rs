use std::path::Path;
use std::path::PathBuf;
use std::fs;
use std::process::Command;

fn compile(shader_dir: &Path, spirv_dir: &PathBuf, source: &str, stage: &str, target: &str) {
    let result = Command::new("glslang")
        .arg("-V")
        .arg("-S")
        .arg(stage)
        .arg("-o")
        .arg(spirv_dir.join(target).display().to_string())
        .arg(shader_dir.join(source).display().to_string())
        .output();
    if !result.unwrap().status.success() {
        panic!("shader compile failed");
    }
}

fn main() {
    println!("cargo:rerun-if-changed=shaders/renderer.vert");
    println!("cargo:rerun-if-changed=shaders/renderer.frag");

    let shader_dir = Path::new("shaders");
    let spirv_dir = shader_dir.join("spirv");
    fs::create_dir_all(spirv_dir.display().to_string()).unwrap();

    compile(shader_dir, &spirv_dir, "renderer.vert", "vert", "renderer_vert.spv");
    compile(shader_dir, &spirv_dir, "renderer.frag", "frag", "renderer_frag.spv");
}
