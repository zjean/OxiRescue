use std::collections::HashSet;
use std::path::PathBuf;

use crate::blob::BlobStore;
use crate::db::schema::*;

pub enum Pane {
    Left,
    Right,
}

pub enum BrowserItem {
    ParentDir,
    Folder(Folder),
    File(FileEntry),
}

pub struct BrowserState {
    pub user_id: String,
    pub user_name: String,
    pub current_folder_id: Option<String>,
    pub current_path: String,
    pub folders: Vec<Folder>,
    pub files: Vec<FileEntry>,
    pub left_selected: usize,
    pub left_items: Vec<BrowserItem>,
    pub selected_items: HashSet<usize>,
    pub target_dir: PathBuf,
    pub right_entries: Vec<String>,
    pub right_selected: usize,
    pub active_pane: Pane,
    pub search_query: Option<String>,
    pub status_message: Option<String>,
}

pub enum Screen {
    Dashboard,
    Browser(BrowserState),
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

    /// Enter the browser screen for a given user, loading root-level content.
    pub fn enter_browser(&mut self, user_id: String, user_name: String) -> anyhow::Result<()> {
        let mut state = BrowserState {
            user_id: user_id.clone(),
            user_name: user_name.clone(),
            current_folder_id: None,
            current_path: "/".to_string(),
            folders: Vec::new(),
            files: Vec::new(),
            left_selected: 0,
            left_items: Vec::new(),
            selected_items: HashSet::new(),
            target_dir: PathBuf::from("/tmp/oxirescue-export"),
            right_entries: Vec::new(),
            right_selected: 0,
            active_pane: Pane::Left,
            search_query: None,
            status_message: None,
        };

        // Load root folders and files
        let folders = self.meta.get_root_folders(&user_id)?;
        let files = self.meta.list_files_in_folder(&user_id, None)?;

        // Build left_items: no ParentDir at root
        let mut items: Vec<BrowserItem> = Vec::new();
        for f in &folders {
            items.push(BrowserItem::Folder(f.clone()));
        }
        for f in &files {
            items.push(BrowserItem::File(f.clone()));
        }

        state.folders = folders;
        state.files = files;
        state.left_items = items;

        self.screen = Screen::Browser(state);

        // Load the right pane
        self.refresh_right_pane();

        Ok(())
    }

    /// Load folders and files for the given folder_id (None = root).
    /// Must be called when screen is Screen::Browser.
    pub fn load_folder(&mut self, folder_id: Option<&str>) -> anyhow::Result<()> {
        if let Screen::Browser(ref mut state) = self.screen {
            let user_id = state.user_id.clone();

            let folders = if let Some(fid) = folder_id {
                self.meta.get_subfolders(fid)?
            } else {
                self.meta.get_root_folders(&user_id)?
            };

            let files = self.meta.list_files_in_folder(&user_id, folder_id)?;

            // Build items list
            let mut items: Vec<BrowserItem> = Vec::new();

            // Only add ParentDir if not at root
            if state.current_folder_id.is_some() {
                items.push(BrowserItem::ParentDir);
            }

            for f in &folders {
                items.push(BrowserItem::Folder(f.clone()));
            }
            for f in &files {
                items.push(BrowserItem::File(f.clone()));
            }

            state.folders = folders;
            state.files = files;
            state.left_items = items;
            state.left_selected = 0;
            state.selected_items.clear();
        }
        Ok(())
    }

    /// Read the target_dir and populate right_entries.
    pub fn refresh_right_pane(&mut self) {
        if let Screen::Browser(ref mut state) = self.screen {
            let target = state.target_dir.clone();
            let mut entries: Vec<String> = Vec::new();

            if let Ok(rd) = std::fs::read_dir(&target) {
                let mut names: Vec<String> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        if e.path().is_dir() {
                            format!("{}/", name)
                        } else {
                            name
                        }
                    })
                    .collect();
                names.sort();
                entries = names;
            }

            state.right_entries = entries;
            if state.right_selected >= state.right_entries.len() && !state.right_entries.is_empty()
            {
                state.right_selected = state.right_entries.len() - 1;
            }
        }
    }
}
