use crate::api::ApiService;

pub struct Auth {
    api_service: Box<ApiService>,
}

impl Auth {
    pub fn new(api_service: Box<ApiService>) -> Self {
        Auth { api_service }
    }

    pub fn execute(&self) {
        self.api_service.auth();
    }
}
