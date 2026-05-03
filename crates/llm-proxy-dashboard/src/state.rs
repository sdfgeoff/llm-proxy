use std::sync::Arc;

use llm_proxy_core::{Config, MasterKey};
use llm_proxy_db::Database;

#[derive(Clone)]
pub struct DashboardState {
    pub(crate) config: Arc<Config>,
    pub(crate) database: Database,
    pub(crate) master_key: MasterKey,
    pub(crate) setup_token: Option<String>,
}

impl DashboardState {
    pub fn new(
        config: Arc<Config>,
        database: Database,
        master_key: MasterKey,
        setup_token: Option<String>,
    ) -> Self {
        Self {
            config,
            database,
            master_key,
            setup_token,
        }
    }
}
