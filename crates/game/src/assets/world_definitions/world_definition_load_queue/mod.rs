//! Bounded definition loading driven by Bevy load-task completion, not frames.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};

use super::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

pub(super) fn request_world_definition_handles_without_per_asset_frame_waits(
    server: AssetServer,
    paths: Vec<PathBuf>,
    completed_loads: Arc<AtomicUsize>,
) -> Task<Vec<Handle<WorldDefinitionAsset>>> {
    // At most four sources are inflated and translated concurrently.
    const MAXIMUM_ACTIVE_DEFINITION_LOADS: usize = 4;
    let paths: Arc<[PathBuf]> = paths.into();
    let next_path_index = Arc::new(AtomicUsize::new(0));
    let workers: [_; MAXIMUM_ACTIVE_DEFINITION_LOADS] = std::array::from_fn(|_| {
        let paths = Arc::clone(&paths);
        let next_path_index = Arc::clone(&next_path_index);
        let server = server.clone();
        let completed_loads = Arc::clone(&completed_loads);
        IoTaskPool::get().spawn(async move {
            let mut handles = Vec::new();
            loop {
                let index = next_path_index.fetch_add(1, Ordering::Relaxed);
                let Some(path) = paths.get(index) else {
                    break;
                };
                let (load_guard, completion) = async_channel::bounded::<()>(1);
                let handle = server
                    .load_builder()
                    .with_guard(load_guard)
                    .load::<WorldDefinitionAsset>(path.clone());
                // Bevy drops the only sender after its load task completes or
                // fails. Channel closure wakes this worker without a frame.
                let _ = completion.recv().await;
                completed_loads.fetch_add(1, Ordering::Relaxed);
                handles.push((index, handle));
            }
            handles
        })
    });
    IoTaskPool::get().spawn(async move {
        let mut handles = Vec::with_capacity(paths.len());
        for worker in workers {
            handles.extend(worker.await);
        }
        // Completion order must not change definition precedence.
        handles.sort_unstable_by_key(|(index, _)| *index);
        handles.into_iter().map(|(_, handle)| handle).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        app::TaskPoolPlugin,
        asset::{
            io::{memory::MemoryAssetReader, AssetSourceBuilder, AssetSourceId, Reader},
            AssetApp, AssetLoader, AssetPlugin, LoadContext,
        },
    };
    use std::{
        io,
        path::Path,
        time::{Duration, Instant},
    };

    #[derive(TypePath)]
    struct DefinitionQueueTestLoader {
        active: Arc<AtomicUsize>,
        maximum_active: Arc<AtomicUsize>,
    }

    impl AssetLoader for DefinitionQueueTestLoader {
        type Asset = WorldDefinitionAsset;
        type Settings = ();
        type Error = io::Error;

        async fn load(
            &self,
            _: &mut dyn Reader,
            _: &(),
            context: &mut LoadContext<'_>,
        ) -> Result<Self::Asset, Self::Error> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum_active.fetch_max(active, Ordering::SeqCst);
            futures_lite::future::yield_now().await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            if context.path().path() == Path::new("7.queue-test") {
                return Err(io::Error::other("intentional queue failure"));
            }
            Ok(WorldDefinitionAsset::from_test_document(Default::default()))
        }

        fn extensions(&self) -> &[&str] {
            &["queue-test"]
        }
    }

    #[test]
    fn definition_queue_preserves_order_and_advances_after_failures_without_app_frames(
    ) -> io::Result<()> {
        let reader = MemoryAssetReader::default();
        let paths = (0..32)
            .map(|index| PathBuf::from(format!("{index}.queue-test")))
            .collect::<Vec<_>>();
        for path in &paths {
            reader.root.insert_asset_text(path, "definition");
        }
        let active = Arc::new(AtomicUsize::new(0));
        let maximum_active = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.register_asset_source(
            AssetSourceId::Default,
            AssetSourceBuilder::new(move || Box::new(reader.clone())),
        )
        .add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<WorldDefinitionAsset>()
        .register_asset_loader(DefinitionQueueTestLoader {
            active: Arc::clone(&active),
            maximum_active: Arc::clone(&maximum_active),
        });
        app.update();
        let server = app.world().resource::<AssetServer>().clone();
        let mut task = request_world_definition_handles_without_per_asset_frame_waits(
            server.clone(),
            paths.clone(),
            Arc::new(AtomicUsize::new(0)),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let handles = loop {
            if let Some(handles) = bevy::tasks::block_on(futures_lite::future::poll_once(&mut task))
            {
                break handles;
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("definition queue waited for an app frame"));
            }
            std::thread::yield_now();
        };
        assert_eq!(handles.len(), paths.len());
        for (handle, path) in handles.iter().zip(&paths) {
            assert_eq!(handle.path().map(|path| path.path()), Some(path.as_path()));
        }
        assert!((1..=4).contains(&maximum_active.load(Ordering::SeqCst)));
        assert_eq!(active.load(Ordering::SeqCst), 0);
        // Publication to Assets still belongs to Bevy's main-world schedule.
        app.update();
        assert!(matches!(
            server.get_load_state(handles[7].id()),
            Some(bevy::asset::LoadState::Failed(_))
        ));
        assert_eq!(
            app.world().resource::<Assets<WorldDefinitionAsset>>().len(),
            31
        );
        Ok(())
    }
}
