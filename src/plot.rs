use plotters::prelude::*;

use chrono::{serde::ts_seconds, DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use std::{collections::HashMap, error::Error};

use crate::json::BenchData;

// TODO: Plot throughput as well as timings
pub fn generate_plots(data: &Plots) -> Result<(), Box<dyn Error>> {
    for plot in data.0.iter() {
        println!("Plotting: {} {:?}", plot.0, plot.1);
        let out_file_name = format!("./{}.png", plot.0);
        let root = BitMapBackend::new(&out_file_name, (1024, 768)).into_drawing_area();
        root.fill(&WHITE)?;

        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .caption(plot.0, ("sans-serif", 40))
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 40)
            .build_cartesian_2d(
                // Add one day buffer before and after
                plot.1
                    .x_axis
                    .min
                    .checked_sub_signed(Duration::days(1))
                    .expect("DateTime underflow")
                    ..plot
                        .1
                        .x_axis
                        .max
                        .checked_add_signed(Duration::days(1))
                        .expect("DateTime overflow"),
                // Add 0.2 ns buffer before and after (not rigorous, based on a priori knowledge of Y axis units & values)
                plot.1.y_axis.min - 0.2f64..plot.1.y_axis.max + 0.2f64,
            )?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .x_label_formatter(&|x| format!("{}", x.format("%m/%d/%y")))
            .x_labels(10)
            .max_light_lines(4)
            .x_desc("Commit Date")
            .y_desc("Time (ns)")
            .draw()?;

        // Draws the lines of benchmark data points, one line/color per set of bench ID params e.g. `rc=100`
        for (i, line) in plot.1.lines.iter().enumerate() {
            // Draw lines between each point
            chart
                .draw_series(LineSeries::new(line.1.iter().map(|p| (p.x, p.y)), style(i)))?
                .label(line.0)
                // TODO: Move the legend out of the plot area
                .legend(move |(x, y)| {
                    Rectangle::new([(x - 5, y - 5), (x + 5, y + 5)], style(i).filled())
                });

            // Draw points and text labels (Git commit) on each point
            chart.draw_series(PointSeries::of_element(
                line.1.iter(),
                5,
                style(i).filled(),
                &|coord, size, style| {
                    EmptyElement::at((coord.x, coord.y))
                        + Circle::new((0, 0), size, style)
                        + Text::new(format!("{:?}", coord.label), (0, 0), ("sans-serif", 15))
                },
            ))?;

            chart
                .configure_series_labels()
                .background_style(WHITE)
                .border_style(BLACK)
                .draw()?;
        }

        // To avoid the IO failure being ignored silently, we manually call the present function
        root.present().expect("Unable to write result to file");
        println!("Result has been saved to {}", out_file_name);
    }

    Ok(())
}

fn style(idx: usize) -> PaletteColor<Palette99> {
    Palette99::pick(idx)
}

// Plots of benchmark results over time/Git history. This data structure is persistent between runs,
// saved to disk in `plot-data.json`, and is meant to be append-only to preserve historical results.
//
// Note:
// Plots are separated by benchmark group and function e.g. `Fibonacci-num=100-Prove`. It doesn't reveal much
// information to view multiple benchmark input results on the same graph (e.g. fib-10 and fib-20),
// since they are expected to be different. Instead, we group different benchmark parameters
// (e.g. `rc` value) onto the same graph to compare/contrast their impact on performance.
#[derive(Debug, Serialize, Deserialize)]
pub struct Plots(HashMap<String, Plot>);

impl Plots {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    // Converts a list of deserialized Criterion benchmark results into a plotting-friendly format,
    // and adds the data to the `Plots` struct.
    pub fn add_data(&mut self, bench_data: &Vec<BenchData>) {
        for bench in bench_data {
            let id = &bench.id;
            let point = Point {
                x: id.params.commit_timestamp,
                y: bench.result.time,
                label: id.params.commit_hash.clone(),
            };
            // plotters doesn't like `/` char in plot title so we use `-`
            let plot_name = format!("{}-{}", id.group_name, id.bench_name);

            if self.0.get(&plot_name).is_none() {
                self.0.insert(plot_name.clone(), Plot::new());
            }
            let plot = self.0.get_mut(&plot_name).unwrap();

            plot.x_axis.set_min_max(id.params.commit_timestamp);
            plot.y_axis.set_min_max(point.y);

            if plot.lines.get(&id.params.params).is_none() {
                plot.lines.insert(id.params.params.to_owned(), vec![]);
            }
            plot.lines.get_mut(&id.params.params).unwrap().push(point);
        }
        // Sort each data point in each line for each plot
        for plot in self.0.iter_mut() {
            for line in plot.1.lines.iter_mut() {
                line.1.sort_by(|a, b| a.partial_cmp(b).unwrap());
            }
        }
    }
}

// The data type for a plot: contains the range of X and Y values, and the line(s) to be drawn
#[derive(Debug, Serialize, Deserialize)]
pub struct Plot {
    x_axis: XAxisRange,
    y_axis: YAxisRange,
    lines: HashMap<String, Vec<Point>>,
}

impl Plot {
    pub fn new() -> Self {
        Self {
            x_axis: XAxisRange::default(),
            y_axis: YAxisRange::default(),
            lines: HashMap::new(),
        }
    }
}

// Historical benchmark result, showing the performance at a given Git commit
#[derive(Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
pub struct Point {
    // Commit timestamp associated with benchmark
    x: DateTime<Utc>,
    // Benchmark time (avg.)
    y: f64,
    // Commit hash (short)
    label: String,
}

// Min. and max. X axis values for a given plot
#[derive(Debug, Serialize, Deserialize)]
pub struct XAxisRange {
    #[serde(with = "ts_seconds")]
    min: DateTime<Utc>,
    #[serde(with = "ts_seconds")]
    max: DateTime<Utc>,
}

// Starts with flipped min/max so they can be set by `Point` values as they are encountered
impl Default for XAxisRange {
    fn default() -> Self {
        Self {
            min: Utc::now(),
            max: chrono::DateTime::<Utc>::MIN_UTC,
        }
    }
}

// Min. and max. Y axis values for a given plot
#[derive(Debug, Serialize, Deserialize)]
pub struct YAxisRange {
    min: f64,
    max: f64,
}

// Starts with flipped min/max so they can be set by `Point` values as they are encountered
impl Default for YAxisRange {
    fn default() -> Self {
        Self {
            min: f64::MAX,
            max: f64::MIN,
        }
    }
}

// Checks if input is < the current min and/or > current max
// If so, sets input as the new min and/or max respectively
trait MinMax<T: PartialOrd> {
    fn set_min_max(&mut self, value: T);
}

impl MinMax<DateTime<Utc>> for XAxisRange {
    fn set_min_max(&mut self, value: DateTime<Utc>) {
        if value < self.min {
            self.min = value
        }
        if value > self.max {
            self.max = value
        }
    }
}

impl MinMax<f64> for YAxisRange {
    fn set_min_max(&mut self, value: f64) {
        if value < self.min {
            self.min = value
        }
        if value > self.max {
            self.max = value
        }
    }
}
