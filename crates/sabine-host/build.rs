use std::{
    env,
    fmt::Write,
    path::{Path, PathBuf},
};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("shared");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    let mut table = String::from("pub(crate) const HOST_FILES: &[(&str, &[u8])] = &[\n");
    for path in files {
        let name = path
            .strip_prefix(&root)
            .unwrap()
            .components()
            .map(|component| component.as_os_str().to_str().unwrap())
            .collect::<Vec<_>>()
            .join("/");
        let path = path.to_str().unwrap();
        writeln!(table, "    ({name:?}, include_bytes!({path:?})),").unwrap();
    }
    table.push_str("];\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("host_files.rs");
    std::fs::write(output, table).unwrap();
}

fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}
