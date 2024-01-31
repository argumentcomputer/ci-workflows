use core::fmt;
use std::io::Read;
use std::{fs::File, path::Path};

use serde::Deserialize;
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
    pub params: String,
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
            Err(serde::de::Error::custom("Expected 3 bench ID elements"))
        } else {
            let bench_name = id[1].replace('_', ":");
            Ok(BenchId {
                group_name: id[0].to_owned(),
                // Criterion converts `:` to `_` in the timestamp as the former is valid JSON syntax,
                // so we convert `_` back to `:` when deserializing
                bench_name,
                params: id[2].to_owned(),
            })
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BenchResult {
    #[serde(rename = "estimate")]
    pub time: f64,
}

// Deserializes the benchmark JSON file into structured data for plotting
pub fn read_json_from_file<P: AsRef<Path>>(path: P) -> Result<Vec<BenchData>, Error> {
    let mut file = File::open(path).unwrap();
    let mut s = String::new();
    file.read_to_string(&mut s).unwrap();

    let mut data = vec![];
    for result in ResilientStreamDeserializer::<BenchData>::new(&s).flatten() {
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
