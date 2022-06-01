use crate::api::ApiService;

pub struct Auth {
    api_service: Box<dyn ApiService>,
}

impl Auth {
    pub fn new(api_service: Box<dyn ApiService>) -> Self {
        Auth { api_service }
    }

    pub fn execute(&self) {
        self.api_service.auth();
    }
}
