mod json;
mod plot;

use std::{
    io::{self, Read, Write},
    path::PathBuf,
};

use anyhow::anyhow;
use json::read_json_from_file;

use crate::plot::{generate_plots, Plots};

// TODO: Switch to camino
// Gets all JSON paths in the current directory, optionally ending in a given suffix
// E.g. if `suffix` is `abc1234.json` it will return "*abc1234.json"
fn get_json_paths(suffix: Option<&str>) -> std::io::Result<Vec<std::path::PathBuf>> {
    let suffix = suffix.unwrap_or(".json");
    let entries = std::fs::read_dir(".")?
        .flatten()
        .filter_map(|e| {
            let ext = e.path();
            if ext.to_str()?.ends_with(suffix) {
                Some(ext)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    Ok(entries)
}

// Benchmark files to plot, e.g. `LURK_BENCH_FILES=fibonacci-abc1234,fibonacci-def5678`
fn bench_files_env() -> anyhow::Result<Vec<String>> {
    std::env::var("LURK_BENCH_FILES")
        .map_err(|e| anyhow!("Benchmark files env var isn't set: {e}"))
        .and_then(|commits| {
            let vec: anyhow::Result<Vec<String>> = commits
                .split(',')
                .map(|sha| {
                    sha.parse::<String>()
                        .map_err(|e| anyhow!("Failed to parse Git commit string: {e}"))
                })
                .collect();
            vec
        })
}

// Deserializes JSON file into `Plots` type
fn read_plots_from_file() -> Result<Plots, io::Error> {
    let path = std::path::Path::new("plot-data.json");

    let mut file = std::fs::File::open(path)?;

    let mut s = String::new();
    file.read_to_string(&mut s)?;

    let plots: Plots = serde_json::from_str(&s)?;

    Ok(plots)
}

// Serializes `Plots` type into file
fn write_plots_to_file(plot_data: &Plots) -> Result<(), io::Error> {
    let path = std::path::Path::new("plot-data.json");

    let mut file = std::fs::File::create(path)?;

    let json_data = serde_json::to_string(&plot_data)?;

    file.write_all(json_data.as_bytes())
}

fn main() {
    // If existing plot data is found on disk, only read and add benchmark files specified by `LURK_BENCH_FILES`
    // Data is stored in a `HashMap` so duplicates are ignored
    let (mut plots, bench_files) = {
        if let Ok(plots) = read_plots_from_file() {
            // The user should know which files they just benchmarked and want to add to the plot
            // Otherwise defaults to all files containing the current Git commit
            let bench_files = bench_files_env().map_or_else(
                |_| {
                    let mut short_sha = env!("VERGEN_GIT_SHA").to_owned();
                    short_sha.truncate(7);
                    get_json_paths(Some(&format!("{}.json", short_sha)))
                        .expect("Failed to read JSON paths")
                },
                |files| {
                    files
                        .iter()
                        .map(|file| PathBuf::from(format!("{}.json", file)))
                        .collect()
                },
            );
            (plots, bench_files)
        }
        // If no plot data exists, read all `JSON` files in the current directory and save to disk
        else {
            let paths = get_json_paths(None).expect("Failed to read JSON paths");
            (Plots::new(), paths)
        }
    };
    println!("Adding bench files to plot: {:?}", bench_files);
    let mut bench_data = vec![];
    for file in bench_files {
        let mut data = read_json_from_file(file).expect("JSON serde error");
        bench_data.append(&mut data);
    }
    plots.add_data(&bench_data);

    // Write to disk
    write_plots_to_file(&plots).expect("Failed to write `Plots` to `plot-data.json`");
    generate_plots(&plots).unwrap();
}
