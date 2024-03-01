use core::fmt;
use std::io::Read;
use std::{fs::File, path::Path};

use anyhow::{anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{de, Deserialize};
use serde_json::de::{StrRead, StreamDeserializer};
use serde_json::{Deserializer, Error, Value};

#[derive(Debug, Deserialize)]
pub struct BenchData {
    pub id: BenchId,
    #[serde(rename = "typical")]
    pub result: BenchResult,
}

#[derive(Debug)]
pub struct BenchId {
    pub group_name: String,
    pub bench_name: String,
    pub params: BenchParams,
}

// Assumes three `String` elements in a Criterion bench ID: <group>/<name>/<params>
// E.g. `Fibonacci-num=10/28db40f-2024-01-30T19:07:04-05:00/rc=100`
// Errors if a different format is found
impl<'de> Deserialize<'de> for BenchId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let id = s.split('/').collect::<Vec<&str>>();
        if id.len() != 3 {
            Err(de::Error::custom("Expected 3 bench ID elements"))
        } else {
            Ok(BenchId {
                group_name: id[0].to_owned(),
                bench_name: id[1].to_owned(),
                params: BenchParams::try_from(id[2])
                    .map_err(|e| de::Error::custom(format!("{}", e)))?,
            })
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct BenchParams {
    pub commit_hash: String,
    pub commit_timestamp: DateTime<Utc>,
    pub params: String,
}

impl TryFrom<&str> for BenchParams {
    type Error = anyhow::Error;
    // Splits a <commit-hash>-<commit-date>-<params> input into a (String, `DateTime`, String) object
    // E.g. `dd2a8e6-2024-02-20T22:48:21-05:00-rc=100` becomes ("dd2a8e6", `<DateTime>`, "rc=100")
    fn try_from(value: &str) -> anyhow::Result<Self> {
        let (commit_hash, rest) = value
            .split_once('-')
            .ok_or_else(|| anyhow!("Invalid format for bench params"))?;
        let arr: Vec<&str> = rest.split_inclusive('-').collect();
        // Criterion converts `:` to `_` in the timestamp as the former is valid JSON syntax,
        // so we convert `_` back to `:` when deserializing
        let mut date: String = arr[..4]
            .iter()
            .flat_map(|s| s.chars())
            .collect::<String>()
            .replace('_', ":");
        date.pop();
        let params = arr[4..].iter().flat_map(|s| s.chars()).collect();

        let commit_timestamp = DateTime::parse_from_rfc3339(&date).map_or_else(
            |e| bail!("Failed to parse string into `DateTime`: {}", e),
            |dt| Ok(dt.with_timezone(&Utc)),
        )?;
        Ok(Self {
            commit_hash: commit_hash.to_owned(),
            commit_timestamp,
            params,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct BenchResult {
    #[serde(rename = "estimate")]
    pub time: f64,
}

// Deserializes the benchmark JSON file into structured data for plotting
pub fn read_json_from_file<P: AsRef<Path>, T: for<'de> Deserialize<'de>>(
    path: P,
) -> Result<Vec<T>, Error> {
    let mut file = File::open(path).unwrap();
    let mut s = String::new();
    file.read_to_string(&mut s).unwrap();

    let mut data = vec![];
    for result in ResilientStreamDeserializer::<T>::new(&s).flatten() {
        data.push(result);
    }
    Ok(data)
}

// The following code is taken from https://users.rust-lang.org/t/step-past-errors-in-serde-json-streamdeserializer/84228/10
// The `ResilientStreamDeserializer` is a workaround to enable a `StreamDeserializer` to continue parsing when it encounters
// a deserialization type error or invalid JSON. See https://github.com/serde-rs/json/issues/70 for discussion
#[derive(Debug)]
pub struct JsonError {
    error: Error,
    value: Option<Value>, // Some(_) if JSON was syntactically valid
}

impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.error)?;

        if let Some(value) = self.value.as_ref() {
            write!(formatter, ", value: {}", value)?;
        }

        Ok(())
    }
}

impl std::error::Error for JsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

pub struct ResilientStreamDeserializer<'de, T> {
    json: &'de str,
    stream: StreamDeserializer<'de, StrRead<'de>, T>,
    last_ok_pos: usize,
}

impl<'de, T> ResilientStreamDeserializer<'de, T>
where
    T: Deserialize<'de>,
{
    pub fn new(json: &'de str) -> Self {
        let stream = Deserializer::from_str(json).into_iter();
        let last_ok_pos = 0;

        ResilientStreamDeserializer {
            json,
            stream,
            last_ok_pos,
        }
    }
}

impl<'de, T> Iterator for ResilientStreamDeserializer<'de, T>
where
    T: Deserialize<'de>,
{
    type Item = Result<T, JsonError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.stream.next()? {
            Ok(value) => {
                self.last_ok_pos = self.stream.byte_offset();
                Some(Ok(value))
            }
            Err(error) => {
                // If an error happened, check whether it's a type error, i.e.
                // whether the next thing in the stream was at least valid JSON.
                // If so, return it as a dynamically-typed `Value` and skip it.
                let err_json = &self.json[self.last_ok_pos..];
                let mut err_stream = Deserializer::from_str(err_json).into_iter::<Value>();
                let value = err_stream.next()?.ok();
                let next_pos = if value.is_some() {
                    self.last_ok_pos + err_stream.byte_offset()
                } else {
                    self.json.len() // when JSON has a syntax error, prevent infinite stream of errors
                };
                self.json = &self.json[next_pos..];
                self.stream = Deserializer::from_str(self.json).into_iter();
                self.last_ok_pos = 0;
                Some(Err(JsonError { error, value }))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::json::BenchParams;
    use chrono::{DateTime, Utc};

    #[test]
    fn parse_bench_params() {
        let s = "dd2a8e6-2024-02-20T22:48:21-05:00-rc=100";
        let params = BenchParams::try_from(s).unwrap();
        let params_expected = BenchParams {
            commit_hash: "dd2a8e6".into(),
            commit_timestamp: DateTime::parse_from_rfc3339("2024-02-20T22:48:21-05:00")
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap(),
            params: "rc=100".into(),
        };
        assert_eq!(params, params_expected);
    }
}
