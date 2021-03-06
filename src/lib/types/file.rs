use crate::lang::execution_context::{ExecutionContext, This, ArgumentVector};
use crate::lang::errors::{CrushResult, to_crush_error};
use crate::lang::r#struct::Struct;
use crate::lang::value::Value;
use std::fs::metadata;
use std::os::unix::fs::MetadataExt;
use lazy_static::lazy_static;
use ordered_map::OrderedMap;
use crate::lang::command::Command;
use crate::lang::command::TypeMap;
use crate::lang::command::OutputType::Unknown;
use crate::lang::command::OutputType::Known;
use crate::lang::value::ValueType;

fn full(name: &'static str) -> Vec<&'static str> {
    vec!["global", "types", "file", name]
}

lazy_static! {
    pub static ref METHODS: OrderedMap<String, Command> = {
        let mut res: OrderedMap<String, Command> = OrderedMap::new();
        res.declare(full("stat"),
            stat, true,
            "file:stat",
            "Return a struct with information about a file.",
            Some(r#"    The return value contains the following fields:

    * is_directory:bool is the file is a directory
    * is_file:bool is the file a regular file
    * is_symlink:bool is the file a symbolic link
    * inode:integer the inode number of the file
    * nlink:integer the number of hardlinks to the file
    * mode:integer the permission bits for the file
    * len: integer the size of the file"#), Unknown);

        res.declare(full("exists"),
            exists, true,
            "file:exists",
            "Return true if this file exists",
            None,
            Known(ValueType::Bool));
        res.declare(full("__getitem__"),
            getitem, true,
            "file[name:string]",
            "Return a file or subdirectory in the specified base directory",
            None,
            Known(ValueType::File));
        res
    };
}

pub fn stat(context: ExecutionContext) -> CrushResult<()> {
    let file = context.this.file()?;
    let metadata = to_crush_error(metadata(file))?;
    context.output.send(
        Value::Struct(
            Struct::new(
                vec![
                    ("is_directory".to_string(), Value::Bool(metadata.is_dir())),
                    ("is_file".to_string(), Value::Bool(metadata.is_file())),
                    ("is_symlink".to_string(), Value::Bool(metadata.file_type().is_symlink())),
                    ("inode".to_string(), Value::Integer(metadata.ino() as i128)),
                    ("nlink".to_string(), Value::Integer(metadata.nlink() as i128)),
                    ("mode".to_string(), Value::Integer(metadata.mode() as i128)),
                    ("len".to_string(), Value::Integer(metadata.len() as i128)),
                ],
                None,
            )
        )
    )
}

pub fn exists(context: ExecutionContext) -> CrushResult<()> {
    context.output.send(Value::Bool(context.this.file()?.exists()))
}

pub fn getitem(mut context: ExecutionContext) -> CrushResult<()> {
    let base_directory = context.this.file()?;
    context.arguments.check_len(1)?;
    let sub = context.arguments.string(0)?;
    context.output.send(Value::File(base_directory.join(&sub)))
}
