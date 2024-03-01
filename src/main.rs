// This crate provides a CLI for plotting and managing historical Criterion benchmark files
// It consists of the following commands:
// - `cargo run plot` will plot the benchmark JSON file(s) at a given path. The bench files are required to be in the
// `BenchOutputType::GhPages` format. See `BenchOutputType` for the different bench ID schemas.
// TODO: below
// - `cargo run convert` will convert benchmark JSONs to different formats for use in other tools.
// E.g. `cargo run convert --input gh-pages --output commit-comment` will reformat the bench ID and other attributes,
// which will enable the benchmark to be used with `criterion-table`.

// NOTE: This tool is only intended for non-regression benchmarks, which compare performance for the *same* functions
// between Git commits over time. In future we may generalize this crate for comparison between different functions.

mod json;
mod plot;

use std::io::{self, Read, Write};

use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Parser, Subcommand, ValueEnum};
use json::read_json_from_file;

use crate::{
    json::BenchData,
    plot::{generate_plots, Plots},
};

/// Criterion benchmark JSON formatter & plotter
#[derive(Parser, Debug)]
#[clap(version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Plot benchmark file(s)
    Plot(PlotArgs),
    /// Convert a benchmark file from one output format to another
    Convert(ConvertArgs),
}

#[derive(Args, Debug)]
struct PlotArgs {
    #[clap(long, value_parser)]
    dir: Option<Utf8PathBuf>,
}

impl PlotArgs {
    fn create_plots(&self) {
        // If existing plot data is found on disk, only read and add benchmark files specified by `LURK_BENCH_FILES`
        // Data is stored in a `HashMap` so duplicates are ignored
        let (mut plots, bench_files) = {
            if let Ok(plots) = read_plots_from_file() {
                // Adds all JSONs contained in `self.dir` to the plot
                // Otherwise defaults to all files in workdir with current Git commit in the filename
                let (dir, suffix) = if let Some(dir) = &self.dir {
                    (dir.as_path(), None)
                } else {
                    let mut commit_hash = env!("VERGEN_GIT_SHA").to_owned();
                    commit_hash.truncate(7);
                    (Utf8Path::new("."), Some(commit_hash))
                };
                let bench_files =
                    get_json_paths(dir, suffix.as_deref()).expect("Failed to read JSON paths");

                (plots, bench_files)
            }
            // If no plot data exists, read all `JSON` files in the current directory and save to disk
            else {
                let paths = get_json_paths(&Utf8PathBuf::from("."), None)
                    .expect("Failed to read JSON paths");
                (Plots::new(), paths)
            }
        };
        println!("Adding bench files to plot: {:?}", bench_files);
        let mut bench_data = vec![];
        for file in bench_files {
            let mut data = read_json_from_file::<_, BenchData>(file).expect("JSON serde error");
            bench_data.append(&mut data);
        }
        plots.add_data(&bench_data);

        // Write to disk
        write_plots_to_file(&plots).expect("Failed to write `Plots` to `plot-data.json`");
        generate_plots(&plots).unwrap();
    }
}

#[derive(Args, Debug)]
struct ConvertArgs {
    /// Bench format of the input
    #[clap(long, value_enum)]
    input: BenchOutputType,

    /// Desired bench format of the output
    #[clap(long, value_enum)]
    output: BenchOutputType,
}

#[derive(Clone, Debug, ValueEnum)]
enum BenchOutputType {
    GhPages,
    CommitComment,
    PrComment,
}

// Deserializes JSON file into `Plots` type
fn read_plots_from_file() -> Result<Plots, io::Error> {
    let path = Utf8Path::new("plot-data.json");

    let mut file = std::fs::File::open(path)?;

    let mut s = String::new();
    file.read_to_string(&mut s)?;

    let plots: Plots = serde_json::from_str(&s)?;

    Ok(plots)
}

// Serializes `Plots` type into file
fn write_plots_to_file(plot_data: &Plots) -> Result<(), io::Error> {
    let path = Utf8Path::new("plot-data.json");

    let mut file = std::fs::File::create(path)?;

    let json_data = serde_json::to_string(&plot_data)?;

    file.write_all(json_data.as_bytes())
}

// Searches for all JSON paths in the specified directory, optionally ending in a given suffix
// E.g. if `suffix` is `abc1234.json` it will return "*abc1234.json"
fn get_json_paths(dir: &Utf8Path, suffix: Option<&str>) -> std::io::Result<Vec<Utf8PathBuf>> {
    let suffix = suffix.unwrap_or(".json");
    let entries = std::fs::read_dir(dir)?
        .flatten()
        .filter_map(|e| {
            let ext = e.path();
            let ext = ext.to_str()?;
            if ext.ends_with(suffix) && ext != "./plot-data.json" {
                Some(Utf8PathBuf::from(ext))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    Ok(entries)
}

fn main() {
    let cli = Cli::parse();
    match &cli.command {
        Command::Plot(p) => p.create_plots(),
        Command::Convert(_c) => todo!(),
    };
}
