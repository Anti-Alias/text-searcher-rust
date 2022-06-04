use std::fs::{File, metadata};
use std::path::{PathBuf, Path};
use std::sync::{Mutex, MutexGuard};

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
    files: Vec<PathBuf>
}

impl State {
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}

impl FinderService {
    
    /// Creates an empty [`FinderService`]
    pub fn new<P: AsRef<Path>>(persist_file: P) -> Self {
        Self {
            persist_file: persist_file.as_ref().to_owned(),
            state: Mutex::new(State { files: Vec::new() })
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

    /// Persists state to a file
    pub fn persist(&self) -> Result<(), PersistErr> {
        let file = File::options()
            .create(true)
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
        files.push(filename.to_owned());
        log::debug!("Added file {}", filename.display());
    }

    fn add_dir<P: AsRef<Path>>(&self, dirname: P) {
        let files = &mut self.state.lock().unwrap().files;
        for entry in WalkDir::new(dirname).into_iter().flat_map(|entry| entry.ok()) {
            if entry.metadata().unwrap().is_file() {
                files.push(entry.path().to_owned());
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

    use crate::persistence::FinderService;

    #[test]
    fn test_add_file_single() {
        let service = FinderService::new("persist-file.json");
        let result = service.add_file("test_files/file.txt");
        let state = service.state();
        
        assert!(result.is_ok());
        assert_eq!(
            &[PathBuf::from("test_files/file.txt")],
            state.files()
        );
    }

    #[test]
    fn test_add_file_dir() {
        let service = FinderService::new("persist-file.json");
        let result = service.add_file("test_files/dir");
        let state = service.state();
        
        assert!(result.is_ok());
        assert_eq!(
            &[
                PathBuf::from("test_files/dir/sub_file_1.txt"),
                PathBuf::from("test_files/dir/sub_file_2.txt")
            ],
            state.files()
        );
    }

    #[test]
    fn test_remove_file_single() {
        let service = FinderService::new("persist-file.json");
        service.add_file("test_files/dir");
        service.remove_files("test_files/dir/sub_file_1.txt");
        let state = service.state();
        assert_eq!(
            &[PathBuf::from("test_files/dir/sub_file_2.txt")],
            state.files()
        );
    }

    #[test]
    fn test_remove_file_multi() {
        let service = FinderService::new("persist-file.json");
        service.add_file("test_files/file.txt");
        service.add_file("test_files/dir");
        service.remove_files("test_files/dir");
        let state = service.state();
        assert_eq!(
            &[PathBuf::from("test_files/file.txt")],
            state.files()
        );
    }
}