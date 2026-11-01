use runbook_core::{model::ProjectCreate, store::directory, Store};
use serde_json::json;
use std::path::Path;

fn add(store: &Store, path: &Path, name: &str) -> String {
    store
        .create_project(ProjectCreate {
            id: None,
            name: name.into(),
            path: path.to_string_lossy().into_owned(),
            description: "保留描述".into(),
            starred: true,
        })
        .unwrap()
        .id
}

#[test]
fn opens_closest_ancestor_and_restores_without_losing_commands() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("db")).unwrap();
    let root = temp.path().join("项目 with spaces");
    let nested = root.join("nested");
    let leaf = nested.join("src");
    std::fs::create_dir_all(&leaf).unwrap();
    let outer = add(&store, &root, "外层项目");
    let inner = add(&store, &nested, "内层项目");
    runbook_core::dispatch(
        &store,
        serde_json::from_value(json!({
            "op":"command_create", "project":inner,
            "input":{"id":"keep","name":"保留命令","base":"echo keep"}
        }))
        .unwrap(),
    )
    .unwrap();
    let before = store.get(&inner).unwrap();
    let (same, created) = store
        .open_project_directory(&leaf.to_string_lossy())
        .unwrap();
    assert!(!created);
    assert_eq!(same.id, inner);
    assert_eq!(
        same.revision, before.revision,
        "reuse does not rewrite active projects"
    );
    runbook_core::dispatch(
        &store,
        serde_json::from_value(json!({
            "op":"project_update", "id":inner, "patch":{"archived":true}
        }))
        .unwrap(),
    )
    .unwrap();
    let archived = store.get(&inner).unwrap();
    let (restored, created) = store
        .open_project_directory(&nested.to_string_lossy())
        .unwrap();
    assert!(!created);
    assert_eq!(restored.id, inner);
    assert!(!restored.archived);
    assert_eq!(restored.revision, archived.revision + 1);
    assert_eq!(restored.name, "内层项目");
    assert_eq!(restored.description, "保留描述");
    assert!(restored.starred);
    assert_eq!(restored.commands[0].id, "keep");
    assert_eq!(store.get(&outer).unwrap().revision, 1);
    assert_eq!(store.list().unwrap().len(), 2);
}

#[test]
fn creates_only_outside_registered_roots_and_rejects_missing_or_file_paths() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("db")).unwrap();
    let root = temp.path().join("app");
    let sibling = temp.path().join("app-other");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&sibling).unwrap();
    add(&store, &root, "app");
    let (created, is_new) = store
        .open_project_directory(&sibling.to_string_lossy())
        .unwrap();
    assert!(
        is_new,
        "a shared string prefix is not a containing directory"
    );
    assert_eq!(created.name, "app-other");
    assert_eq!(created.path, directory(&sibling.to_string_lossy()).unwrap());
    assert!(created.commands.is_empty());
    assert!(created.description.is_empty());
    let (same, is_new) = store
        .open_project_directory(&sibling.join(".").to_string_lossy())
        .unwrap();
    assert!(!is_new);
    assert_eq!(same.id, created.id);
    #[cfg(windows)]
    {
        let (same, is_new) = store
            .open_project_directory(&sibling.to_string_lossy().to_uppercase())
            .unwrap();
        assert!(!is_new);
        assert_eq!(same.id, created.id);
    }
    let file = temp.path().join("file");
    std::fs::write(&file, "keep").unwrap();
    for path in [file, temp.path().join("missing")] {
        assert_eq!(
            store
                .open_project_directory(&path.to_string_lossy())
                .unwrap_err()
                .code,
            "VALIDATION_ERROR"
        );
    }
    assert_eq!(store.list().unwrap().len(), 2);
    assert!(!temp.path().join("missing").exists());
}

#[test]
fn concurrent_directory_openers_create_exactly_one_project() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("db")).unwrap();
    let path = temp.path().join("新项目");
    std::fs::create_dir(&path).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
    let handles: Vec<_> = (0..6)
        .map(|_| {
            let store = store.clone();
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store
                    .open_project_directory(&path.to_string_lossy())
                    .unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|(_, created)| *created).count(), 1);
    assert!(results.iter().all(|(p, _)| p.id == results[0].0.id));
    assert_eq!(store.list().unwrap().len(), 1);
}
