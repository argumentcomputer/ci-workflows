use plotters::prelude::*;

use chrono::{serde::ts_seconds, DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use std::{collections::HashMap, error::Error};

use crate::json::BenchData;

// TODO: Figure out how to include the commit hash as a label on the point or X-axis
pub fn generate_plots(data: &Plots) -> Result<(), Box<dyn Error>> {
    for plot in data.0.iter() {
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
            .x_labels(10)
            .max_light_lines(4)
            .x_desc("Commit Date")
            .y_desc("Time (ns)")
            .draw()?;

        // Draws the lines of benchmark data points, one line/color per set of bench ID params e.g. `rc=100`
        for (i, line) in plot.1.lines.iter().enumerate() {
            // Draw lines between each point
            chart
                .draw_series(LineSeries::new(
                    line.1.iter().map(|p| (p.x, p.y)),
                    Palette99::pick(i),
                ))?
                .label(line.0)
                // TODO: Move the legend out of the plot area
                .legend(move |(x, y)| {
                    Rectangle::new(
                        [(x - 5, y - 5), (x + 5, y + 5)],
                        Palette99::pick(i).filled(),
                    )
                });

            // Draw dots on each point
            chart.draw_series(
                line.1
                    .iter()
                    .map(|p| Circle::new((p.x, p.y), 3, Palette99::pick(i).filled())),
            )?;
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

// Convert <short-sha>-<commit-date> to a `DateTime` object, discarding `short-sha`
fn str_to_datetime(input: &str) -> Result<DateTime<Utc>, Box<dyn Error>> {
    // Removes the first 8 chars (assuming UTF8) for the `short-sha` and trailing '-'
    let datetime: &str = input.split_at(8).1;

    DateTime::parse_from_rfc3339(datetime).map_or_else(
        |e| Err(format!("Failed to parse string into `DateTime`: {}", e).into()),
        |dt| Ok(dt.with_timezone(&Utc)),
    )
}

// Plots of benchmark results over time/Git history. This data structure is persistent between runs,
// saved to disk in `plot-data.json`, and is meant to be append-only to preserve historical results.
//
// Note:
// Plots are separated by benchmark input e.g. `Fibonacci-num-100`. It doesn't reveal much
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
            let commit_date = str_to_datetime(&bench.id.bench_name).expect("Timestamp parse error");
            let point = Point {
                x: commit_date,
                y: bench.result.time,
            };

            if self.0.get(&bench.id.group_name).is_none() {
                self.0.insert(bench.id.group_name.to_owned(), Plot::new());
            }
            let plot = self.0.get_mut(&bench.id.group_name).unwrap();

            plot.x_axis.set_min_max(commit_date);
            plot.y_axis.set_min_max(point.y);

            if plot.lines.get(&bench.id.params).is_none() {
                plot.lines.insert(bench.id.params.to_owned(), vec![]);
            }
            plot.lines.get_mut(&bench.id.params).unwrap().push(point);
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
