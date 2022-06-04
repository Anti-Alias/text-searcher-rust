use std::path::{PathBuf, Path};
use std::ffi::{OsString};
use std::fs::File;
use std::io::BufReader;
use csv::Writer;

use clap::{arg, Command};
use text_searcher_rust::{Finder, Phrase, Text};
use threadpool::ThreadPool;
use walkdir::WalkDir;

fn main() {
    // Parses args
    let matches = Command::new("Binary Text Searcher")
        .version("1.0")
        .about("Searches for text in binary files")
        .arg(arg!(-f --file <VALUE> ...))
        .arg(arg!(-p --phrase <VALUE> ...))
        .arg(arg!(-c --context_size <VALUE> ...))
        .arg(arg!(-w --window_size <VALUE> ...))
        .arg(arg!(-e --extension <VALUE> ...).required(false))
        .arg(arg!(-t --threads <VALUE>).required(false).default_value("8"))
        .get_matches();

    // gets files listed as path buffers
    let files: Vec<PathBuf> = matches
        .values_of("file")
        .expect("--file not specified")
        .map(|file_str| PathBuf::from(file_str))
        .collect();

    // gets files listed as path buffers
    let phrases: Vec<Phrase> = matches
        .values_of("phrase")
        .expect("--phrase not specified")
        .map(|phrase_str| phrase_str
            .split(" ")
            .map(|text_str| Text::from_str(text_str))
            .collect::<Vec<Text>>()
        )
        .map(|text_vec| Phrase(text_vec))
        .collect();

    // Gets optional extension
    let extensions: Option<Vec<OsString>> = matches
        .values_of("extension")
        .map(|values| {
            values.map(|ext| OsString::from(ext)).collect()
        });

    // Gets context size
    let context_size: usize = matches
        .value_of("context_size")
        .unwrap()
        .parse()
        .unwrap();

    // Gets window size
    let window_size: usize = matches
        .value_of("window_size")
        .unwrap()
        .parse()
        .unwrap();

    // Gets number of threads
    let threads: usize = matches.value_of("threads")
        .unwrap()
        .parse()
        .unwrap();

    // Stores non-directory files that whos extensions are in `extensions`
    let extensions = extensions
        .as_ref()
        .map(|list| list.as_slice());
    let mut files_recursive: Vec<PathBuf> = Vec::new();
    get_files_recursive(&mut files_recursive, &files, extensions);

    // Processes expanded files
    let pool = ThreadPool::new(threads);
    for file in files_recursive {
        let phrases = phrases.clone();
        pool.execute(move || {
            process(file, phrases.as_slice(), context_size, window_size).unwrap();
        });
    }
    pool.join()
}

// Takes all files in `src` expands them recursively, and places all non-directory files in `dest`.
// If `extensions` is specified, only includes files with said extensions.
// Otherwise, includes all of them.
fn get_files_recursive(
    dest: &mut Vec<PathBuf>,
    src: &[PathBuf],
    extensions: Option<&[OsString]>
) {
    for path in src {
        for entry in WalkDir::new(path) {
            match entry {
                Ok(entry) => {
                    let is_file = entry.file_type().is_file();
                    if is_file && has_extension(entry.path(), extensions) {
                        dest.push(entry.path().into())
                    }
                }
                _ => {}
            }
        }
    }
}

fn has_extension(file: &Path, extensions: Option<&[OsString]>) -> bool {
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

fn process(
    path: impl AsRef<Path>,
    phrases: &[Phrase],
    context_size: usize,
    window_size: usize
) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut finder = Finder::new(phrases, context_size, window_size, &mut reader);
    let mut next = finder.next();
    let mut writer = Writer::from_writer(std::io::stdout());
    writer.write_record(&[
        "file",
        "phrase",
        "file_pos",
        "codepoint_diff",
        "bytes_per_character",
        "context"
    ]).unwrap();
    while let Some(group) = next {
        for instance in group.0 {
            let phrase = &phrases[instance.phrase_index];
            let bbc = instance.bytes_per_character;
            let cpd = instance.codepoint_diff;
            let ctx = finder.get_context(cpd, bbc);
            writer.write_record(&[
                &path.display().to_string(),
                &phrase.to_string(),
                &finder.bytes_read().to_string(),
                &cpd.to_string(),
                &bbc.to_string(),
                &ctx.to_string()
            ]).unwrap();
        }
        next = finder.next();
    }
    Ok(())
}