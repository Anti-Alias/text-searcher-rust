use std::path::{PathBuf, Path};
use std::ffi::{OsString};
use std::fs::File;
use std::io::BufReader;

use clap::{arg, Command};
use text_searcher_rust::{Finder, Phrase, Text};
use walkdir::WalkDir;

fn main() {
    // Parses args
    let matches = Command::new("Binary Text Searcher")
        .version("1.0")
        .about("Searches for text in binary files")
        .arg(arg!(-f --file <VALUE> ...))
        .arg(arg!(-p --phrase <VALUE> ...))
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

    // Gets number of threads
    let _threads: u32 = matches.value_of("threads")
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
    for file in files_recursive {
        process(file, phrases.as_slice()).unwrap();
    }
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

fn process(path: impl AsRef<Path>, phrases: &[Phrase]) -> Result<(), std::io::Error> {
    let context_size = 64;
    let window_size = 32;
    let path = path.as_ref();
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut finder = Finder::new(phrases, context_size, window_size, &mut reader);
    let mut next = finder.next();
    while let Some(group) = next {
        for instance in group.0 {
            let phrase = &phrases[instance.phrase_index];
            let bbc = instance.bytes_per_character;
            let cpd = instance.codepoint_diff;
            let ctx = finder.get_context(cpd, bbc);
            let spacing = "             ";
            let mut underline = String::new();
            let underline_idx = instance.file_pos - finder.get_context_range().start;
            for _ in 0..underline_idx {
                underline.push(' ');
            }
            underline.push('-');
            println!(
                "File:       '{}'\nphrase:     '{}'\nFile pos:   {}\ncontext:    '{}'\n{}{}\n",
                path.display(),
                phrase,
                instance.file_pos,
                ctx,
                spacing,
                underline
            );
        }
        next = finder.next();
    }
    Ok(())
}