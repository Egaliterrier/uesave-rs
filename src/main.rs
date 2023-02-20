use std::fs::{File, OpenOptions};
use std::io::{stdin, stdout, BufRead, BufReader, BufWriter, Cursor, Write};

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

use uesave::Save;

#[derive(Parser, Debug)]
struct IO {
    #[arg(short, long, default_value = "-")]
    input: String,

    #[arg(short, long, default_value = "-")]
    output: String,
}

#[derive(Parser, Debug)]
struct Edit {
    #[arg(required = true, index = 1)]
    path: String,

    #[arg(short, long)]
    editor: Option<String>,
}

#[derive(Parser, Debug)]
struct ActionTestResave {
    #[arg(required = true, index = 1)]
    path: String,

    /// If resave fails, write input.sav and output.sav to working directory for debugging
    #[arg(short, long)]
    debug: bool,
}

#[derive(Subcommand, Debug)]
enum Action {
    /// Convert binary save to plain text JSON
    ToJson(IO),
    /// Convert JSON back to binary save
    FromJson(IO),
    /// Launch $EDITOR to edit a save file as JSON in place
    Edit(Edit),
    /// Test resave
    TestResave(ActionTestResave),
}

#[derive(Parser, Debug)]
#[command(author, version)]
struct Args {
    #[command(subcommand)]
    action: Action,
}

pub fn main() -> Result<()> {
    let args = Args::parse();

    match args.action {
        Action::ToJson(io) => {
            let save = Save::read(&mut input(&io.input)?)?;
            serde_json::to_writer_pretty(output(&io.output)?, &save)?;
        }
        Action::FromJson(io) => {
            let save: Save = serde_json::from_reader(&mut input(&io.input)?)?;
            save.write(&mut output(&io.output)?)?;
        }
        Action::TestResave(action) => {
            let mut input = std::io::Cursor::new(std::fs::read(action.path)?);
            let mut output = std::io::Cursor::new(vec![]);
            Save::read(&mut input)?.write(&mut output)?;
            let (input, output) = (input.into_inner(), output.into_inner());
            if input != output {
                if action.debug {
                    std::fs::write("input.sav", input)?;
                    std::fs::write("output.sav", output)?;
                }
                return Err(anyhow!("Resave did not match"));
            }
            println!("Resave successful");
        }
        Action::Edit(edit) => {
            let editor = match edit.editor {
                Some(editor) => editor,
                None => std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string()),
            };

            // read and parse save file
            let buffer = std::fs::read(&edit.path)?;
            let save = Save::read(&mut Cursor::new(&buffer))?;
            let value = serde_json::to_value(save)?;

            // create temp file and write formatted JSON to it
            let temp = tempfile::Builder::new().suffix(".json").tempfile()?;
            serde_json::to_writer_pretty(BufWriter::new(&temp), &value)?;

            // launch editor
            let mut args = shell_words::split(&editor)
                .expect("failed to parse EDITOR")
                .into_iter();
            std::process::Command::new(args.next().expect("EDITOR empty"))
                .args(args)
                .arg("--")
                .arg(temp.path())
                .stdin(std::process::Stdio::piped())
                .spawn()?
                .wait()?;

            // rebuild save if modified
            let modified_save: Save = serde_json::from_reader(BufReader::new(temp.reopen()?))?;
            let mut out_buffer = vec![];
            modified_save.write(&mut out_buffer)?;
            if buffer == out_buffer {
                println!("File unchanged, doing nothing.");
            } else {
                println!("File modified, writing new save.");
                OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(edit.path)?
                    .write_all(&out_buffer)?;
            }
        }
    }
    Ok(())
}

fn input<'a>(path: &str) -> Result<Box<dyn BufRead + 'a>> {
    Ok(match path {
        "-" => Box::new(BufReader::new(stdin().lock())),
        p => Box::new(BufReader::new(File::open(p)?)),
    })
}

fn output<'a>(path: &str) -> Result<Box<dyn Write + 'a>> {
    Ok(match path {
        "-" => Box::new(BufWriter::new(stdout().lock())),
        p => Box::new(BufWriter::new(
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(p)?,
        )),
    })
}
