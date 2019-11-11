use std::path::Path;

use chrono::{Local};
use regex::Regex;

use crate::closure::Closure;
use crate::commands::JobJoinHandle;
use crate::data::{Cell, Output, ListDefinition, Row, Rows};
use crate::env::Env;
use crate::errors::{error, mandate, JobResult, argument_error, to_job_error, JobError};
use crate::glob::Glob;
use crate::job::Job;
use crate::printer::Printer;
use crate::stream::streams;
use std::time::Duration;
use crate::data::row::RowWithTypes;

#[derive(Clone)]
#[derive(Debug)]
pub enum CellDefinition {
    Text(Box<str>),
    Integer(i128),
    Time(Vec<CellDefinition>),
    Duration(Vec<CellDefinition>),
    Field(Vec<Box<str>>),
    Glob(Glob),
    Regex(Box<str>, Regex),
    Op(Box<str>),
    ClosureDefinition(Closure),
    JobDefintion(Job),
    MaterializedJobDefintion(Job),
    File(Box<Path>),
    Variable(Vec<Box<str>>),
    List(ListDefinition),
    Subscript(Box<CellDefinition>, Box<CellDefinition>),
}

fn to_duration(a: u64, t: &str) -> JobResult<Duration> {
    match t {
        "nanosecond" | "nanoseconds" => Ok(Duration::from_nanos(a)),
        "microsecond" | "microseconds" => Ok(Duration::from_micros(a)),
        "millisecond" | "milliseconds" => Ok(Duration::from_millis(a)),
        "second" | "seconds" => Ok(Duration::from_secs(a)),
        "minute" | "minutes" => Ok(Duration::from_secs(a*60)),
        "hour" | "hours" => Ok(Duration::from_secs(a*3600)),
        "day" | "days" => Ok(Duration::from_secs(a*3600*24)),
        "year" | "years" => Ok(Duration::from_secs(a*3600*24*365)),
        _ => Err(error("Invalid duration"))
    }
}

fn compile_duration_mode(cells: &Vec<CellDefinition>, dependencies: &mut Vec<JobJoinHandle>, env: &Env, printer: &Printer) -> JobResult<Cell> {
    let v: Vec<Cell> = cells.iter()
        .map(|c| c.compile(dependencies, env, printer))
        .collect::<JobResult<Vec<Cell>>>()?;
    let duration = match &v[..] {
        [Cell::Integer(s)] => Duration::from_secs(*s as u64),
        [Cell::Time(t1), Cell::Text(operator), Cell::Time(t2)] => if operator.as_ref() == "-" {
            to_job_error(t1.signed_duration_since(t2.clone()).to_std())?
        } else {
            return Err(error("Illegal duration"))
        },
        _ => if v.len() % 2 == 0 {
            let vec: Vec<Duration> = v.chunks(2)
                .map(|chunk| match (&chunk[0], &chunk[1]) {
                    (Cell::Integer(a), Cell::Text(t)) => to_duration(*a as u64, t.as_ref()),
                    _ => Err(argument_error("Unknown duration format"))
                })
                .collect::<JobResult<Vec<Duration>>>()?;
            vec.into_iter().sum::<Duration>()
        } else {
            return Err(error("Unknown duration format"))
        },
    };

    Ok(Cell::Duration(duration))
}

fn compile_time_mode(cells: &Vec<CellDefinition>, dependencies: &mut Vec<JobJoinHandle>, env: &Env, printer: &Printer) -> JobResult<Cell> {
    let v: Vec<Cell> = cells.iter()
        .map(|c | c.compile(dependencies, env, printer))
        .collect::<JobResult<Vec<Cell>>>()?;
    let time = match &v[..] {
        [Cell::Text(t)] => if t.as_ref() == "now" {Local::now()} else {return Err(error("Unknown time"))},
        _ => return Err(error("Unknown duration format")),
    };

    Ok(Cell::Time(time))
}

impl CellDefinition {
    pub fn compile(&self, dependencies: &mut Vec<JobJoinHandle>, env: &Env, printer: &Printer) -> JobResult<Cell> {
        Ok(match self {
            CellDefinition::Text(v) => Cell::Text(v.clone()),
            CellDefinition::Integer(v) => Cell::Integer(v.clone()),
            CellDefinition::Time(v) => compile_time_mode(v, dependencies, env, printer)?,
            CellDefinition::Duration(c) => compile_duration_mode(c, dependencies, env, printer)?,
            CellDefinition::Field(v) => Cell::Field(v.clone()),
            CellDefinition::Glob(v) => Cell::Glob(v.clone()),
            CellDefinition::Regex(v, r) => Cell::Regex(v.clone(), r.clone()),
            CellDefinition::Op(v) => Cell::Op(v.clone()),
            CellDefinition::File(v) => Cell::File(v.clone()),
            CellDefinition::JobDefintion(def) => {
                let (first_output, first_input) = streams();
                first_output.initialize(vec![])?;
                let (last_output, last_input) = streams();
                let j = def.spawn_and_execute(&env, printer, first_input, last_output)?;

                let res = Cell::Output(Output { stream: last_input.initialize()? });
                dependencies.push(j);
                res
            }
            CellDefinition::MaterializedJobDefintion(def) => {
                let (first_output, first_input) = streams();
                first_output.initialize(vec![])?;
                let (last_output, last_input) = streams();
                let j = def.spawn_and_execute(&env, printer, first_input, last_output)?;
                let res = Cell::Output(Output { stream: last_input.initialize()? }).materialize();
                dependencies.push(j);
                res
            }
            CellDefinition::ClosureDefinition(c) => Cell::Closure(c.with_env(env)),
            CellDefinition::Variable(s) => (
                mandate(
                    env.get(s),
                    format!("Unknown variable").as_str())?),
            CellDefinition::List(l) => l.compile(dependencies, env, printer)?,
            CellDefinition::Subscript(c, i) => {
                match (c.compile(dependencies, env, printer), i.compile(dependencies, env, printer)) {
                    (Ok(Cell::List(list)), Ok(Cell::Integer(idx))) =>
                        list.get(idx as usize)?,
                    (Ok(Cell::Dict(dict)), Ok(c)) =>
                        mandate(dict.get(&c), "Invalid subscript")?,
                    (Ok(Cell::Env(env)), Ok(Cell::Text(name))) =>
                        mandate(env.get_str(name.as_ref()), "Invalid subscript")?,
                    (Ok(Cell::Row(row)), Ok(Cell::Text(col))) =>
                        mandate(row.get(col.as_ref()), "Invalid subscript")?,
                    (Ok(Cell::Output(o)), Ok(Cell::Integer(idx))) => {
                        Cell::Row(RowWithTypes {
                            types: o.stream.get_type().clone(),
                            cells: o.get(idx)?.cells
                        })
                    }
                    _ => return Err(error("Expected a list variable")),
                }
            }
        })
    }

    pub fn text(s: &str) -> CellDefinition {
        CellDefinition::Text(Box::from(s))
    }

    pub fn op(s: &str) -> CellDefinition {
        CellDefinition::Op(Box::from(s))
    }

    pub fn regex(s: &str, r: Regex) -> CellDefinition {
        CellDefinition::Regex(Box::from(s), r)
    }
}
