use crate::database::DatabaseHandle;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseHandle,
}
