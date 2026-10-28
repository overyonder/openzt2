use super::super::behavior_snapshot_types::{
    BehaviorActorSnapshot, BehaviorFrameSnapshot, BehaviorSnapshotEntityReference,
};
use crate::persistence::persistence_failure_types::WorldSnapshotPersistenceFailure;
use bevy::{asset::LoadState, prelude::*};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_zoo_state::behavior_task_execution_types::{
    BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, BehaviorTaskReturnFrame,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn resolve_behavior_frame(
    frame: &BehaviorFrameSnapshot,
    documents: &Assets<BehaviorDocumentAsset>,
    handles: &BTreeMap<String, Handle<BehaviorDocumentAsset>>,
) -> Result<BehaviorTaskReturnFrame, WorldSnapshotPersistenceFailure> {
    let handle = handles
        .get(&frame.document)
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    let document = documents
        .get(handle)
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    let declaration = (0..)
        .map_while(|index| {
            if frame.phase == BehaviorTaskExecutionPhase::Set {
                document
                    .behavior_set_at_index(index)
                    .map(|set| (index, set.id, &set.actions, &set.subjects))
            } else {
                document.behavior_task_at_index(index).map(|task| {
                    (
                        index,
                        task.id,
                        match frame.phase {
                            BehaviorTaskExecutionPhase::Execution => &task.execution,
                            BehaviorTaskExecutionPhase::Completion => &task.completion,
                            BehaviorTaskExecutionPhase::Failure => &task.failure,
                            BehaviorTaskExecutionPhase::Set => unreachable!(),
                        },
                        &task.subjects,
                    )
                })
            }
        })
        .filter(|(_, program, _, subjects)| {
            if *program != frame.program {
                return false;
            }
            let mut subjects = subjects
                .iter()
                .map(|subject| openzt2_game_data::AssetId::from_key(subject))
                .collect::<Vec<_>>();
            subjects.sort_unstable();
            subjects.dedup();
            subjects == frame.subjects
        })
        .last()
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    if frame.action > declaration.2.lowered_actions().len() {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(BehaviorTaskReturnFrame {
        program: frame.program,
        document: handle.clone(),
        declaration: declaration.0,
        phase: frame.phase,
        action: frame.action,
        repetitions: frame.repetitions,
    })
}

pub(super) fn actor_references(
    record: &BehaviorActorSnapshot,
) -> impl Iterator<Item = &BehaviorSnapshotEntityReference> {
    record.task.target.iter().chain(&record.task.object)
        .chain(record.membership.iter().map(|membership| &membership.container))
        .chain(record.water_target.iter().map(|water| &water.region))
        .chain(record.staff_job.iter().map(|job| &job.target))
        .chain(record.synchronized.iter().map(|owner| &owner.target))
        .chain(record.navigation.iter().flat_map(|nav| {
            nav.pursuit_target.iter()
                .chain(nav.docking.iter().map(|dock| &dock.target))
                .chain(nav.docking_completion.iter().filter_map(|pending| pending.target.as_ref()))
                .chain(nav.pending_results.iter().filter_map(|result| match result {
                    super::super::behavior_snapshot_types::BehaviorNavigationResultSnapshot::Arrived { target, .. } => target.as_ref(),
                    _ => None,
                }))
        }))
}

fn required_reference(
    reference: &BehaviorSnapshotEntityReference,
) -> BehaviorSnapshotEntityReference {
    match reference {
        BehaviorSnapshotEntityReference::SelfToken { recipient, .. } => {
            BehaviorSnapshotEntityReference::Persistent(*recipient)
        }
        BehaviorSnapshotEntityReference::TankCleaningSite { tank, .. } => {
            BehaviorSnapshotEntityReference::Habitat(*tank)
        }
        reference => reference.clone(),
    }
}

pub(super) fn validate_behavior_snapshot_references(
    records: &[BehaviorActorSnapshot],
    identities: &BTreeMap<BehaviorSnapshotEntityReference, Entity>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    for record in records {
        if !identities.contains_key(&BehaviorSnapshotEntityReference::Persistent(
            record.persistent_identifier,
        )) || actor_references(record)
            .any(|reference| !identities.contains_key(&required_reference(reference)))
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        if let Some(owner) = record
            .synchronized
            .as_ref()
            .filter(|owner| owner.target_outcome.is_none())
        {
            let target_execution = match &owner.target {
                BehaviorSnapshotEntityReference::Persistent(id) => records
                    .binary_search_by_key(id, |record| record.persistent_identifier)
                    .ok()
                    .map(|index| records[index].task.execution_id),
                target => identities.get(target).and_then(|target_entity| {
                    records
                        .iter()
                        .find(|record| {
                            identities.get(&BehaviorSnapshotEntityReference::Persistent(
                                record.persistent_identifier,
                            )) == Some(target_entity)
                        })
                        .map(|record| record.task.execution_id)
                }),
            };
            if target_execution != Some(owner.target_execution_id) {
                return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
            }
        }
    }
    Ok(())
}

pub(super) fn prepare_behavior_snapshot_documents(
    records: &[BehaviorActorSnapshot],
    documents: &Assets<BehaviorDocumentAsset>,
    server: &AssetServer,
    retained: &mut Vec<Handle<BehaviorDocumentAsset>>,
) -> Result<Option<BTreeMap<String, Handle<BehaviorDocumentAsset>>>, WorldSnapshotPersistenceFailure>
{
    let mut paths = BTreeSet::new();
    for record in records {
        paths.insert(record.task.current.document.clone());
        paths.extend(record.task.stack.iter().map(|frame| frame.document.clone()));
        if let Some(job) = &record.staff_job {
            paths.insert(job.document.clone());
        }
        if let Some(owner) = &record.synchronized {
            paths.insert(owner.origin.document.clone());
        }
        if let Some(nav) = &record.navigation {
            paths.extend(
                nav.move_completion
                    .iter()
                    .chain(&nav.docking_completion)
                    .map(|pending| pending.origin.document.clone()),
            );
            paths.extend(
                nav.pursuit
                    .iter()
                    .map(|pending| pending.origin.document.clone()),
            );
        }
    }
    let mut ready = true;
    let mut handles = BTreeMap::new();
    for path in paths {
        let handle: Handle<BehaviorDocumentAsset> = server.load(path.clone());
        if matches!(
            server.get_load_state(handle.id()),
            Some(LoadState::Failed(_))
        ) {
            return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
        }
        if !retained.contains(&handle) {
            retained.push(handle.clone());
        }
        ready &= documents.get(&handle).is_some();
        handles.insert(path, handle);
    }
    if !ready {
        return Ok(None);
    }
    for record in records {
        resolve_behavior_frame(&record.task.current, documents, &handles)?;
        for frame in &record.task.stack {
            resolve_behavior_frame(frame, documents, &handles)?;
        }
        if let Some(owner) = &record.synchronized {
            if !matches!(
                resolve_behavior_frame_action(&owner.origin, documents, &handles)?,
                openzt2_game_data::behavior::action_record::BehaviorAction::SynchronizedSets(_)
            ) {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
        }
        let has_task_template = std::iter::once(&record.task.current)
            .chain(&record.task.stack)
            .any(|frame| frame.phase != BehaviorTaskExecutionPhase::Set);
        if has_task_template
            && record
                .task
                .score
                .as_ref()
                .is_none_or(|score| score.template.is_none())
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        if let Some(nav) = &record.navigation {
            if let Some(completion) = &nav.move_completion {
                if !matches!(
                    resolve_behavior_frame_action(&completion.origin, documents, &handles)?,
                    openzt2_game_data::behavior::action_record::BehaviorAction::Move(_)
                ) {
                    return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
                }
            }
            if let Some(completion) = &nav.docking_completion {
                if !matches!(
                    resolve_behavior_frame_action(&completion.origin, documents, &handles)?,
                    openzt2_game_data::behavior::action_record::BehaviorAction::Dock(_)
                        | openzt2_game_data::behavior::action_record::BehaviorAction::DockQueue(_)
                ) {
                    return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
                }
            }
            if let Some(pursuit) = &nav.pursuit {
                if !matches!(
                    resolve_behavior_frame_action(&pursuit.origin, documents, &handles)?,
                    openzt2_game_data::behavior::action_record::BehaviorAction::Pursuit(_)
                ) {
                    return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
                }
            }
        }
    }
    Ok(Some(handles))
}

fn resolve_behavior_frame_action<'a>(
    frame: &BehaviorFrameSnapshot,
    documents: &'a Assets<BehaviorDocumentAsset>,
    handles: &BTreeMap<String, Handle<BehaviorDocumentAsset>>,
) -> Result<
    &'a openzt2_game_data::behavior::action_record::BehaviorAction,
    WorldSnapshotPersistenceFailure,
> {
    let frame = resolve_behavior_frame(frame, documents, handles)?;
    let source = documents
        .get(&frame.document)
        .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
    let probe = BehaviorTaskExecutionState {
        execution_id: 0,
        program: frame.program,
        document: frame.document,
        declaration: frame.declaration,
        phase: frame.phase,
        action: frame.action,
        repetitions: frame.repetitions,
        target: None,
        object: None,
        next_action_tick: 0,
        interaction_slot: None,
        stack: Default::default(),
    };
    openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action(
        source, &probe,
    )
    .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
}
