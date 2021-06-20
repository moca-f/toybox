use std::any::Any;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;

use errors::*;
use tb_core::serde::de::DeserializeOwned;
use tb_ecs::*;

use crate::path::TbPath;

pub mod entity_instance;
pub mod prefab;

mod errors {
    pub use tb_core::error::*;

    error_chain! {}
}

#[derive(Copy, Clone)]
pub struct AssetHandle<T> {
    id: u64,
    _phantom: PhantomData<T>,
}

pub struct AssetLoader {
    id_to_assets: HashMap<u64, Box<dyn Any + Send>>,
    path_to_ids: HashMap<PathBuf, u64>,
    loading_pool: thread_pool::ThreadPool,
    completed_assets_sender: std::sync::mpsc::Sender<(u64, Result<Box<dyn Any + Send>>)>,
    completed_assets_receiver: std::sync::mpsc::Receiver<(u64, Result<Box<dyn Any + Send>>)>,
    next_id: u64,
}

///
/// # Safety
///
/// Don't use `completed_assets_sender` and `completed_assets_receiver` in immutable methods
unsafe impl Sync for AssetLoader {}

impl AssetLoader {
    pub fn load<T: 'static + Any + Send + for<'de> serde::Deserialize<'de>>(
        &mut self,
        path: TbPath,
    ) -> AssetHandle<T> {
        let id = match self.path_to_ids.entry(path.into()) {
            Entry::Occupied(occupied) => *occupied.get(),
            Entry::Vacant(vacant) => {
                let id = self.next_id;
                self.next_id += 1;
                let sender = self.completed_assets_sender.clone();
                let path = vacant.key().clone();
                self.loading_pool.execute(move || {
                    sender.send(Self::load_block::<T>(id, &path)).unwrap();
                });
                vacant.insert(id);
                id
            }
        };

        AssetHandle {
            id,
            _phantom: Default::default(),
        }
    }

    pub fn update(&mut self) -> Result<()> {
        loop {
            let asset = match self.completed_assets_receiver.try_recv() {
                Ok(asset) => asset,
                Err(e) => match e {
                    TryRecvError::Empty => {
                        break;
                    }
                    TryRecvError::Disconnected => {
                        return Err(Error::with_chain(
                            e,
                            "AssetLoader::completed_assets_receiver disconnected",
                        ));
                    }
                },
            };

            let (id, asset) = (
                asset.0,
                match asset.1 {
                    Ok(asset) => asset,
                    Err(e) => {
                        eprintln!("{}", e.display_chain());
                        continue;
                    }
                },
            );

            assert!(self.id_to_assets.insert(id, asset).is_none());
        }

        Ok(())
    }

    pub fn get<T: 'static>(&self, handle: AssetHandle<T>) -> Option<&T> {
        self.id_to_assets
            .get(&handle.id)
            .map(|asset| asset.downcast_ref().unwrap())
    }

    fn load_block<T: 'static + Send + DeserializeOwned>(
        id: u64,
        path: &Path,
    ) -> (u64, Result<Box<dyn Any + Send>>) {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) => {
                return (
                    id,
                    Err(Error::with_chain(
                        e,
                        format!("Failed to open asset file. path: {:?}", path),
                    )),
                )
            }
        };

        let res: T = match serde_json::from_reader(file) {
            Ok(res) => res,
            Err(e) => {
                return (
                    id,
                    Err(Error::with_chain(
                        e,
                        format!("Failed to deserialize asset. path: {:?}", path),
                    )),
                )
            }
        };
        (id, Ok(Box::new(res)))
    }
}

impl Default for AssetLoader {
    fn default() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        Self {
            id_to_assets: Default::default(),
            path_to_ids: Default::default(),
            loading_pool: Default::default(),
            completed_assets_sender: sender,
            completed_assets_receiver: receiver,
            next_id: 0,
        }
    }
}

#[system]
struct LoadAssetSystem {}

impl<'s> System<'s> for LoadAssetSystem {
    type SystemData = Write<'s, AssetLoader>;

    fn run(&mut self, mut asset_loader: Self::SystemData) {
        if let Some(err) = asset_loader.update().err() {
            eprintln!("{}", err.display_chain());
        }
    }
}