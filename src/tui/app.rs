use crate::blob::BlobStore;
use crate::db::schema::*;

pub enum Screen {
    Dashboard,
    Browser { user_id: String, user_name: String },
}

pub struct App {
    pub meta: Box<dyn MetadataSource>,
    pub blobs: BlobStore,
    pub screen: Screen,
    pub stats: Option<StorageStats>,
    pub users: Vec<(User, u64, u64)>, // user, file_count, total_bytes
    pub selected_user: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(meta: Box<dyn MetadataSource>, blobs: BlobStore) -> Self {
        Self {
            meta,
            blobs,
            screen: Screen::Dashboard,
            stats: None,
            users: Vec::new(),
            selected_user: 0,
            should_quit: false,
        }
    }

    pub fn load_dashboard(&mut self) -> anyhow::Result<()> {
        self.stats = Some(self.meta.stats()?);
        let users = self.meta.list_users()?;
        self.users = users
            .into_iter()
            .map(|u| {
                let (count, bytes) = self.meta.user_stats(&u.id).unwrap_or((0, 0));
                (u, count, bytes)
            })
            .collect();
        Ok(())
    }
}
