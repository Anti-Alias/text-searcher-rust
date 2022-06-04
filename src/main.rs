use std::path::PathBuf;

use rocket::{launch, routes, get, post, State};
use rocket::http::Status;
use rocket::serde::json::Json;

use text_searcher_rust::{Phrase, Text};

use crate::finder_service::FinderService;

pub mod finder_service;

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

#[post("/add-phrase", data = "<phrase>", format = "json")]
fn add_phrase(phrase: Json<String>, finder_service: &State<FinderService>) -> Result<(), Status> {
    let texts: Vec<Text> = phrase.0
        .split_whitespace()
        .map(|text_str| Text::from_str(text_str))
        .collect();
    finder_service.add_phrase(Phrase(texts));
    persist_finder(finder_service)
}

#[post("/remove-phrase", data = "<phrase>", format = "json")]
fn remove_phrase(phrase: Json<String>, finder_service: &State<FinderService>) -> Result<Json<bool>, Status> {
    let texts: Vec<Text> = phrase.0
        .split_whitespace()
        .map(|text_str| Text::from_str(text_str))
        .collect();
    if finder_service.remove_phrase(&Phrase(texts)) {
        persist_finder(finder_service)?;
        Ok(Json(true))
    }
    else {
        Ok(Json(false))
    }
}

#[get("/list-phrases")]
fn list_phrases(finder_service: &State<FinderService>) -> Json<Vec<String>> {
    let state = finder_service.state();
    let phrases: Vec<String> = state
        .phrases()
        .map(|phrase| phrase.to_string())
        .collect();
    Json(phrases)
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
            list_files,
            add_phrase,
            remove_phrase,
            list_phrases
        ])
        .manage(FinderService::new("persist.json"))
}