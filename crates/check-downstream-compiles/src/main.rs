use std::collections::BTreeMap;
use std::fs;

use camino::Utf8PathBuf;
use clap::{Parser, ValueEnum};
use toml_edit::{value, DocumentMut, Item, Table};
use walkdir::WalkDir;

/// CLI to patch a downstream repo and check it compiles
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to upstream crate
    #[arg(long)]
    upstream: String,

    /// Path to downstream crate
    #[arg(long)]
    downstream: String,

    /// The type of patch in `[patch.<patch-type>]`
    #[arg(long, value_enum, default_value_t = PatchType::default())]
    patch_type: PatchType,

    /// The org/repo name to be patched via GitHub URL, e.g. argumentcomputer/sphinx
    #[arg(long)]
    repo: String,
}

#[derive(Debug, Clone, Default, ValueEnum)]
enum PatchType {
    // TODO
    CratesIO,
    Ssh,
    #[default]
    Https,
}

fn main() {
    let args = Args::parse();

    let mut upstream_packages: BTreeMap<String, Utf8PathBuf> = BTreeMap::new();

    // Get all the upstream crates and their paths
    for entry in WalkDir::new(args.upstream)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if let Some(file_name) = path.file_name() {
            if file_name == "Cargo.toml" {
                let dir = path.parent().expect("No parent for Cargo.toml");
                let cargo_toml_content = fs::read_to_string(path).expect("FS err");
                let doc = cargo_toml_content
                    .parse::<DocumentMut>()
                    .expect("Parse err");
                if let Some(package) = doc.get("package") {
                    if let Some(name) = package.get("name") {
                        let dep_name = name.as_str().unwrap().to_string();
                        upstream_packages.insert(
                            dep_name,
                            Utf8PathBuf::from_path_buf(dir.to_owned()).unwrap(),
                        );
                    }
                }
            }
        }
    }

    //println!("upstream packages: {:?}", upstream_packages);

    let mut downstream_packages: BTreeMap<String, Utf8PathBuf> = BTreeMap::new();

    // Get all the upstream crates that are used in the downstream repo
    for entry in WalkDir::new(&args.downstream)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if let Some(file_name) = path.file_name() {
            if file_name == "Cargo.toml" {
                let cargo_toml_content = fs::read_to_string(path).expect("FS err");
                let doc = cargo_toml_content
                    .parse::<DocumentMut>()
                    .expect("Parse err");

                if let Some(Item::Table(deps)) = doc.get("dependencies") {
                    for (dep_name, dep_value) in deps.iter() {
                        if let Some(table) = dep_value.as_inline_table() {
                            if table.get("git").is_some() {
                                if let Some(dir) = upstream_packages.get(dep_name) {
                                    downstream_packages.insert(dep_name.to_owned(), dir.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    //println!("downstream packages: {:?}", downstream_packages);

    let patch_str = patch_string(&args.patch_type, &args.repo);

    // Patch each downstream crate with the upstream crates
    // Iterate through each crate in the downstream workspace
    // Read each Cargo.toml file into toml_edit
    // Write the patches for each patch in downstream_packages
    for entry in WalkDir::new(&args.downstream)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if let Some(file_name) = path.file_name() {
            if file_name == "Cargo.toml" {
                let cargo_toml_content = fs::read_to_string(path).expect("FS err");
                let mut doc = cargo_toml_content
                    .parse::<DocumentMut>()
                    .expect("Parse err");

                // Ensure [patch.<patch-type>] table exists, create it if it doesn't
                if let Some(Item::Table(patch)) = doc.get_mut("patch") {
                    if let Some(Item::Table(patch_table)) = patch.get_mut(&patch_str) {
                        // Add entries to the existing [patch.<patch-type>] table
                        for (pkg, dir) in downstream_packages.iter() {
                            add_patch(patch_table, pkg, dir.as_str());
                        }
                    } else {
                        // Create the [patch.<patch-type>] table and add entries
                        let mut patch_table = Table::new();
                        for (pkg, dir) in downstream_packages.iter() {
                            add_patch(&mut patch_table, pkg, dir.as_str());
                        }
                        patch[&patch_str] = Item::Table(patch_table);
                    }
                } else {
                    // Create the [patch] table, then the [patch.<patch-type>] table and add entries
                    let mut patch = Table::new();
                    patch.set_implicit(true);
                    let mut patch_table = Table::new();
                    for (pkg, dir) in downstream_packages.iter() {
                        add_patch(&mut patch_table, pkg, dir.as_str());
                    }
                    patch[&patch_str] = Item::Table(patch_table);
                    doc["patch"] = Item::Table(patch);
                }

                //println!("file: {:?}\n{doc}", path);

                fs::write(path, doc.to_string()).expect("Failed to write");
            }
        }
    }
}

// TODO: Clean this up with a From/Display impl
fn patch_string(patch_type: &PatchType, repo: &str) -> String {
    match patch_type {
        PatchType::CratesIO => String::from("crates-io"),
        PatchType::Ssh => format!("ssh://git@github.com/{repo}"),
        PatchType::Https => format!("https://github.com/{repo}"),
    }
}

fn add_patch(patch_table: &mut Table, dep: &str, path: &str) {
    patch_table[dep]["path"] = value(path);
    patch_table[dep]
        .as_inline_table_mut()
        .unwrap() //_or_else(|| bail!("Failed to get mutable table for {dep}"))
        .fmt();
}
