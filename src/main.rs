use std::error::Error;
use std::path::PathBuf;

use rocket::{launch, routes, get, post, State};
use rocket::http::Status;
use rocket::serde::json::Json;

use crate::persistence::FinderService;

pub mod persistence;

#[get("/")]
fn index() -> &'static str { "Hello, world!" }

#[post("/add-file/<file_name>")]
fn add_file(file_name: &str, finder_service: &State<FinderService>) -> Result<(), Status> {
    match finder_service.add_file(file_name) {
        Ok(_) => {},
        Err(_) => return Err(Status::NotFound)
    }
    persist_finder(finder_service)
}

#[post("/remove-files/<file_name>")]
fn remove_files(file_name: &str, finder_service: &State<FinderService>) -> Result<(), Status> {
    finder_service.remove_files(file_name);
    persist_finder(finder_service)
}

#[get("/list-files")]
fn list_files(finder_service: &State<FinderService>) -> Json<Vec<PathBuf>> {
    let state = finder_service.state();
    let files: Vec<PathBuf> = state.files().map(|path| path.to_owned()).collect();
    Json(files)
}

//  Helper function that persists the finder service
fn persist_finder(finder_service: &State<FinderService>) -> Result<(), Status> {
    match finder_service.persist() {
        Ok(_) => Ok(()),
        Err(_) => Err(Status::InternalServerError)
    }
}

#[launch]
fn rocket() -> _ {
    env_logger::init();
    rocket::build()
        .mount("/", routes![
            index,
            add_file,
            remove_files,
            list_files
        ])
        .manage(FinderService::new("persist.json"))
}