use crate::api::ApiService;
use std::path::Path;
use std::process::exit;

pub struct Upload {
    api_service: Box<dyn ApiService>,
    filepath: String,
}

impl Upload {
    pub fn new(api_service: Box<dyn ApiService>, filepath: String) -> Self {
        if !Path::new(&filepath).exists() {
            println!("session {} doest not exist", filepath);
            exit(1);
        }

        Upload {
            api_service,
            filepath,
        }
    }

    pub fn execute(&self) {
        match self.api_service.upload(&self.filepath) {
            Some(result_url) => println!("Result Url: {}", result_url),
            _ => {}
        }
    }
}
