use std::collections::HashSet;
use std::fs::{File, metadata};
use std::path::{PathBuf, Path};
use std::sync::{Mutex, MutexGuard};

use text_searcher_rust::Phrase;
use walkdir::WalkDir;
use serde::{Serialize, Deserialize};

/// Service that keeps track of files to monitor for text changes.
pub struct FinderService {
    persist_file: PathBuf,
    state: Mutex<State>
}


// Represents the inner state of a [`FinderService`]
#[derive(Serialize, Deserialize)]
pub struct State {
    files: HashSet<PathBuf>,
    phrases: HashSet<Phrase>
}


impl State {
    pub fn new() -> Self {
        Self {
            files: HashSet::new(),
            phrases: HashSet::new()
        }
    }
    pub fn files(&self) -> impl Iterator<Item=&PathBuf> {
        self.files.iter()
    }
    pub fn phrases(&self) -> impl Iterator<Item=&Phrase> {
        self.phrases.iter()
    }
}

impl FinderService {
    
    /// Creates an empty [`FinderService`]
    pub fn new<P: AsRef<Path>>(persist_file: P) -> Self {
        let persist_file = persist_file.as_ref().to_owned();
        match File::open(&persist_file) {
            Ok(file) => {
                let state = serde_json::from_reader(file).unwrap();
                Self {
                    persist_file,
                    state: Mutex::new(state)
                }
            },
            Err(_) => Self {
                persist_file,
                state: Mutex::new(State::new())
            }
        }
    }

    /// Internal state of the service
    pub fn state(&self) -> MutexGuard<State> {
        self.state.lock().unwrap()
    }

    /// Tracks the file specified.
    /// If filename is a file, only tracks that file.
    /// If filename is a directory, recursively tracks all the files beneath the directory.
    pub fn add_file<P: AsRef<Path>>(&self, filename: P) -> Result<(), std::io::Error> {
        let meta = metadata(&filename)?;
        if meta.is_file() {
            self._add_file(filename);
        }
        else {
            self.add_dir(filename);
        }
        Ok(())
    }

    /// Stops tracking all files that start with the filename prefix, if any.
    pub fn remove_files<P: AsRef<Path>>(&self, filename: P) {
        let files = &mut self.state.lock().unwrap().files;
        files.retain(|file| !file.starts_with(&filename));
    }

    /// Adds a phrase to the service
    pub fn add_phrase(&self, phrase: Phrase) {
        let mut state = self.state.lock().unwrap();
        state.phrases.insert(phrase);
    }

    /// Adds a phrase to the service
    pub fn remove_phrase(&self, phrase: &Phrase) -> bool {
        let mut state = self.state.lock().unwrap();
        state.phrases.remove(&phrase)
    }

    /// Persists state to a file
    pub fn persist(&self) -> Result<(), PersistErr> {
        let file = File::options()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.persist_file);
        let file = match file {
            Ok(file) => file,
            Err(err) => {
                log::error!("Failed to open '{}': {:?}", &self.persist_file.display(), err);
                return Err(PersistErr::IoError(err));
            }
        };
        match serde_json::to_writer(&file, &self.state) {
            Ok(_) => Ok(()),
            Err(err) => Err(PersistErr::JsonError(err))
        }
    }

    fn _add_file<P: AsRef<Path>>(&self, filename: P) {
        let files = &mut self.state.lock().unwrap().files;
        let filename = filename.as_ref();
        files.insert(filename.to_owned());
        log::debug!("Added file {}", filename.display());
    }

    fn add_dir<P: AsRef<Path>>(&self, dirname: P) {
        let files = &mut self.state.lock().unwrap().files;
        for entry in WalkDir::new(dirname).into_iter().flat_map(|entry| entry.ok()) {
            if entry.metadata().unwrap().is_file() {
                files.insert(entry.path().to_owned());
            }
        }
    }
}

#[derive(Debug)]
pub enum PersistErr {
    IoError(std::io::Error),
    JsonError(serde_json::Error)
}


#[cfg(test)]
mod tests {

    use std::path::PathBuf;

    use crate::finder_service::FinderService;

    #[test]
    fn test_add_file_single() {
        let service = FinderService::new("persist-file.json");
        let result = service.add_file("test_files/file.txt");
        let state = service.state();
        let mut files: Vec<PathBuf> = state.files().map(|file| file.to_owned()).collect();
        files.sort();

        assert!(result.is_ok());
        assert_eq!(
            [PathBuf::from("test_files/file.txt")].to_vec(),
            files
        );
    }

    #[test]
    fn test_add_file_dir() {
        let service = FinderService::new("persist-file.json");
        let result = service.add_file("test_files/dir");
        let state = service.state();
        let mut files: Vec<PathBuf> = state.files().map(|file| file.to_owned()).collect();
        files.sort();

        assert!(result.is_ok());
        assert_eq!(
            [
                PathBuf::from("test_files/dir/sub_file_1.txt"),
                PathBuf::from("test_files/dir/sub_file_2.txt")
            ].to_vec(),
            files
        );
    }

    #[test]
    fn test_remove_file_single() {
        let service = FinderService::new("persist-file.json");
        service.add_file("test_files/dir");
        service.remove_files("test_files/dir/sub_file_1.txt");
        let state = service.state();
        let mut files: Vec<PathBuf> = state.files().map(|file| file.to_owned()).collect();
        files.sort();

        assert_eq!(
            [PathBuf::from("test_files/dir/sub_file_2.txt")].to_vec(),
            files
        );
    }

    #[test]
    fn test_remove_file_multi() {
        let service = FinderService::new("persist-file.json");
        service.add_file("test_files/file.txt");
        service.add_file("test_files/dir");
        service.remove_files("test_files/dir");
        let state = service.state();
        let mut files: Vec<PathBuf> = state.files().map(|file| file.to_owned()).collect();
        files.sort();

        assert_eq!(
            [PathBuf::from("test_files/file.txt")].to_vec(),
            files
        );
    }
}