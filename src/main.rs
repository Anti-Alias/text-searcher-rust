use std::{path::{PathBuf, Path}, ffi::{OsStr, OsString}, fs::File, io::BufReader};

use clap::{arg, Command};
use text_searcher_rust::{Finder, Phrase};
use walkdir::WalkDir;

fn main() {
    // Parses args
    let matches = Command::new("Binary Text Searcher")
        .version("1.0")
        .about("Searches for text in binary files")
        .arg(arg!(-f --files <VALUE>))
        .arg(arg!(-e --extensions <VALUE> ...).required(false))
        .arg(arg!(-t --threads <VALUE>).required(false).default_value("8"))
        .get_matches();

    // Extracts args
    let file = matches.value_of("files").expect("--files not specified");
    let extensions: Option<Vec<OsString>> = matches
        .values_of("extensions")
        .map(|values| {
            values.map(|ext| OsString::from(ext)).collect()
        });

    let threads: u32 = matches.value_of("threads")
        .unwrap()
        .parse()
        .unwrap();

    // Stores non-directory files that whos extensions are in `extensions`
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(file) {
        match entry {
            Ok(dir_entry) => {
                let is_file = dir_entry.file_type().is_file();
                if is_file && has_extension(dir_entry.path(), extensions.as_ref()) {
                    files.push(dir_entry.path().into())
                }
            }
            _ => {}
        }
    }

    // Processes files
    for file in files {
        println!("Processing '{}'", file.display());
        process(file);
    }
}

fn has_extension(file: &Path, extensions: Option<&Vec<OsString>>) -> bool {
    let extensions = match extensions {
        Some(ext) => ext,
        None => return true
    };
    let file_ext = match file.extension() {
        Some(ext) => ext,
        None => return false
    };
    extensions
        .iter()
        .map(|ext| ext.as_os_str())
        .any(|ext| ext == file_ext)
}

fn process(path: impl AsRef<Path>) -> Result<(), std::io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let finder = Finder::new(phrases, 256, 256, &mut reader);
    for group in finder {
        for instance in group.0 {

        }
    }
    Ok(())
}