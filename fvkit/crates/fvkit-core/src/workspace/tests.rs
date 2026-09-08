use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    store: Store,
    repo: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "fv-workspace-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let repo = dir.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]).unwrap();
        git(&repo, &["config", "user.name", "Workspace Test"]).unwrap();
        git(
            &repo,
            &["config", "user.email", "workspace@example.invalid"],
        )
        .unwrap();
        fs::write(repo.join("README"), "base\n").unwrap();
        fs::write(repo.join(".gitignore"), "ignored\n").unwrap();
        git(&repo, &["add", "."]).unwrap();
        git(&repo, &["commit", "-m", "base"]).unwrap();
        let root = dir.join("volume");
        fs::create_dir_all(&root).unwrap();
        let store = Store::new(root, dir.join("state"), dir.join("cache"));
        let f = Self { dir, store, repo };
        f.ok(
            "register",
            Action::RegisterRepository(RegisterRepository {
                repository: Some(Repository {
                    id: "desktop".into(),
                    host: "github.com".into(),
                    namespace: "fastverk/subgroup".into(),
                    name: "desktop".into(),
                    path: text(&f.repo).unwrap(),
                    ..Default::default()
                }),
            }),
        );
        f.ok(
            "project",
            Action::PutProject(PutProject {
                project: Some(ProjectSpec {
                    id: "fastverk".into(),
                    repository_ids: vec!["desktop".into()],
                    ..Default::default()
                }),
                expected_revision: 0,
            }),
        );
        f
    }
    fn apply(&self, id: &str, action: Action) -> WorkspaceOperation {
        self.store
            .apply(WorkspaceCommand {
                request_id: id.into(),
                action: Some(action),
            })
            .unwrap()
    }
    fn ok(&self, id: &str, action: Action) -> WorkspaceOperation {
        let o = self.apply(id, action);
        assert_eq!(o.phase, OperationPhase::Succeeded as i32, "{}", o.detail);
        o
    }
    fn provision(&self) -> Action {
        Action::Provision(ProvisionWorkspace {
            id: "task-1".into(),
            project_id: "fastverk".into(),
            display_name: "First task".into(),
            inputs: vec![RepositoryInput {
                repository_id: "desktop".into(),
                base_revision: "main".into(),
            }],
        })
    }
    fn workspace(&self) -> ManagedWorkspace {
        self.store.snapshot().unwrap().workspaces[0].clone()
    }
    fn release(&self) {
        let w = self.workspace();
        self.ok(
            "lease",
            Action::Lease(LeaseWorkspace {
                workspace_id: w.id.clone(),
                owner: "test".into(),
                expected_revision: w.revision,
                ttl_seconds: 60,
                ..Default::default()
            }),
        );
        let w = self.workspace();
        self.ok(
            "release",
            Action::Release(ReleaseWorkspace {
                workspace_id: w.id,
                owner: "test".into(),
                generation: 1,
                expected_revision: w.revision,
                abandonment_reason: "disposable test".into(),
                ..Default::default()
            }),
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn pinned_base_normalized_paths_idempotency_and_build_context() {
    let f = Fixture::new();
    let initial = commit(&f.repo, "main").unwrap();
    git(&f.repo, &["checkout", "-b", "unrelated"]).unwrap();
    fs::write(f.repo.join("README"), "other branch\n").unwrap();
    git(&f.repo, &["commit", "-am", "unrelated"]).unwrap();
    let op = f.ok("provision", f.provision());
    assert_eq!(f.ok("provision", f.provision()), op);
    let w = f.workspace();
    let c = &w.checkouts[0];
    assert_eq!(c.base_commit, initial);
    assert_eq!(commit(Path::new(&c.path), "HEAD").unwrap(), initial);
    assert!(c
        .path
        .ends_with("workspaces/task-1/src/github.com/fastverk/subgroup/desktop"));
    assert!(f.store.root.join("projects/fastverk/project.pb").is_file());
    let mut changed = f.provision();
    if let Action::Provision(p) = &mut changed {
        p.display_name = "different".into();
    }
    assert!(f
        .store
        .apply(WorkspaceCommand {
            request_id: "provision".into(),
            action: Some(changed)
        })
        .is_err());
    let ctx = f.store.build_context("task-1").unwrap();
    assert!(ctx.requires_input_snapshot);
    assert!(!ctx.sources[0].dirty);
    fs::write(Path::new(&c.path).join("new-input"), "dirty").unwrap();
    assert!(f.store.build_context("task-1").unwrap().sources[0].dirty);
}
#[test]
fn path_escapes_and_duplicate_identity_are_blocked() {
    let f = Fixture::new();
    let mut action = f.provision();
    if let Action::Provision(p) = &mut action {
        p.id = "../escape".into();
    }
    assert_eq!(
        f.apply("escape", action).phase,
        OperationPhase::Blocked as i32
    );
    let r = f.store.snapshot().unwrap().repositories[0].clone();
    assert_eq!(
        f.apply(
            "duplicate",
            Action::RegisterRepository(RegisterRepository {
                repository: Some(Repository {
                    id: "another".into(),
                    ..r
                })
            })
        )
        .phase,
        OperationPhase::Blocked as i32
    );
    assert!(!f.store.root.join("escape").exists());
    #[cfg(unix)]
    {
        let target = f.store.root.join("workspaces");
        fs::remove_dir(&target).unwrap();
        std::os::unix::fs::symlink(&f.repo, &target).unwrap();
        assert_eq!(
            f.apply("symlink", f.provision()).phase,
            OperationPhase::Blocked as i32
        );
    }
}
#[test]
fn leases_fence_stale_owners_and_expiration_does_not_release() {
    let f = Fixture::new();
    f.ok("provision", f.provision());
    let w = f.workspace();
    f.ok(
        "owner",
        Action::Lease(LeaseWorkspace {
            workspace_id: w.id.clone(),
            owner: "alice".into(),
            expected_revision: w.revision,
            ttl_seconds: 60,
            ..Default::default()
        }),
    );
    let w = f.workspace();
    assert_eq!(
        f.apply(
            "competing",
            Action::Lease(LeaseWorkspace {
                workspace_id: w.id.clone(),
                owner: "bob".into(),
                expected_revision: w.revision,
                generation: 1,
                ttl_seconds: 60
            })
        )
        .phase,
        OperationPhase::Blocked as i32
    );
    assert_eq!(
        f.apply(
            "stale",
            Action::Release(ReleaseWorkspace {
                workspace_id: w.id.clone(),
                owner: "alice".into(),
                generation: 0,
                expected_revision: w.revision,
                abandonment_reason: "done".into(),
                ..Default::default()
            })
        )
        .phase,
        OperationPhase::Blocked as i32
    );
    let mut state = f.store.snapshot().unwrap();
    state.workspaces[0].lease.as_mut().unwrap().expires_at = 0;
    f.store.save(&mut state).unwrap();
    let inspected = f.store.inspect(&w.id).unwrap();
    assert!(!inspected.collectable);
    assert!(inspected
        .retention_reasons
        .iter()
        .any(|r| r.contains("explicitly released")));
    f.ok(
        "takeover",
        Action::Lease(LeaseWorkspace {
            workspace_id: w.id,
            owner: "bob".into(),
            expected_revision: w.revision,
            generation: 1,
            ttl_seconds: 60,
        }),
    );
    assert_eq!(f.workspace().lease.unwrap().generation, 2);
}
#[test]
fn dirty_ignored_locked_and_runtime_content_are_retained() {
    let f = Fixture::new();
    f.ok("provision", f.provision());
    f.release();
    let w = f.workspace();
    let path = Path::new(&w.checkouts[0].path);
    for (file, reason) in [
        ("untracked", "untracked content"),
        ("ignored", "ignored content"),
    ] {
        fs::write(path.join(file), "valuable").unwrap();
        let inspected = f.store.inspect(&w.id).unwrap();
        assert!(!inspected.collectable);
        assert!(
            inspected
                .retention_reasons
                .iter()
                .any(|r| r.contains(reason)),
            "{:?}",
            inspected.retention_reasons
        );
        fs::remove_file(path.join(file)).unwrap();
    }
    git(&f.repo, &["worktree", "lock", &w.checkouts[0].path]).unwrap();
    assert!(f
        .store
        .inspect(&w.id)
        .unwrap()
        .retention_reasons
        .iter()
        .any(|r| r.contains("locked")));
    git(&f.repo, &["worktree", "unlock", &w.checkouts[0].path]).unwrap();
    fs::write(Path::new(&w.path).join("run/service.log"), "preserve").unwrap();
    assert!(f
        .store
        .inspect(&w.id)
        .unwrap()
        .retention_reasons
        .iter()
        .any(|r| r.contains("unmanaged content")));
    let collect = Action::Collect(CollectWorkspace {
        workspace_id: w.id,
        expected_revision: w.revision,
    });
    assert_eq!(
        f.apply("collect", collect).phase,
        OperationPhase::Blocked as i32
    );
    assert!(path.exists());
}
#[test]
fn journal_lock_and_corruption_fail_closed() {
    let f = Fixture::new();
    let lock = File::options()
        .write(true)
        .open(f.store.state.join("writer.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(f
        .store
        .apply(WorkspaceCommand {
            request_id: "blocked".into(),
            action: Some(f.provision())
        })
        .is_err());
    drop(lock);
    fs::write(f.store.state.join("catalog.pb"), b"corrupt").unwrap();
    assert!(f.store.snapshot().is_err());
}

#[test]
fn multi_repository_recovery_uses_recorded_pins_after_refs_advance() {
    let f = Fixture::new();
    let second = f.dir.join("second");
    git(
        &f.dir,
        &["clone", f.repo.to_str().unwrap(), second.to_str().unwrap()],
    )
    .unwrap();
    f.ok(
        "register-second",
        Action::RegisterRepository(RegisterRepository {
            repository: Some(Repository {
                id: "shared".into(),
                host: "github.com".into(),
                namespace: "another/group".into(),
                name: "desktop".into(),
                path: text(&second).unwrap(),
                ..Default::default()
            }),
        }),
    );
    f.ok(
        "project-update",
        Action::PutProject(PutProject {
            project: Some(ProjectSpec {
                id: "fastverk".into(),
                repository_ids: vec!["desktop".into(), "shared".into()],
                ..Default::default()
            }),
            expected_revision: 1,
        }),
    );
    let mut action = f.provision();
    if let Action::Provision(p) = &mut action {
        p.inputs.push(RepositoryInput {
            repository_id: "shared".into(),
            base_revision: "main".into(),
        });
    }
    f.ok("multi", action.clone());
    let before = f.workspace();
    assert_eq!(before.checkouts.len(), 2);
    git(&second, &["worktree", "remove", &before.checkouts[1].path]).unwrap();
    // Simulate a restart after the first worktree was created and pins saved.
    let mut state = f.store.snapshot().unwrap();
    state.workspaces[0].phase = WorkspacePhase::Provisioning as i32;
    state
        .operations
        .iter_mut()
        .find(|o| o.id == "multi")
        .unwrap()
        .phase = OperationPhase::Running as i32;
    f.store.save(&mut state).unwrap();
    fs::write(f.repo.join("README"), "new main\n").unwrap();
    git(&f.repo, &["commit", "-am", "main advanced"]).unwrap();
    f.ok("multi", action);
    let after = f.workspace();
    assert_eq!(before.checkouts, after.checkouts);
    assert_ne!(
        commit(&f.repo, "main").unwrap(),
        after.checkouts[0].base_commit
    );
    assert_eq!(
        commit(Path::new(&after.checkouts[1].path), "HEAD").unwrap(),
        after.checkouts[1].base_commit
    );
}

#[test]
fn collection_recovers_after_git_removal_and_preserves_branches() {
    let f = Fixture::new();
    f.ok("provision", f.provision());
    f.release();
    let w = f.workspace();
    // On hosts without reliable process observation the contract fails closed.
    if idle(Path::new(&w.path)).is_err() {
        assert!(!f.store.inspect(&w.id).unwrap().collectable);
        return;
    }
    assert!(f.store.inspect(&w.id).unwrap().collectable);
    let mut s = f.store.snapshot().unwrap();
    s.workspaces[0].phase = WorkspacePhase::Removing as i32;
    s.workspaces[0].checkouts[0].removal_started = true;
    f.store.save(&mut s).unwrap();
    git(&f.repo, &["worktree", "remove", &w.checkouts[0].path]).unwrap();
    f.ok(
        "collect",
        Action::Collect(CollectWorkspace {
            workspace_id: w.id,
            expected_revision: w.revision,
        }),
    );
    assert_eq!(f.workspace().phase, WorkspacePhase::Removed as i32);
    assert!(commit(&f.repo, &w.checkouts[0].branch).is_ok());
    assert!(Path::new(&w.path).join("workspace.pb").exists());
}

#[test]
fn normal_collection_and_live_process_protection() {
    let f = Fixture::new();
    f.ok("provision", f.provision());
    f.release();
    let w = f.workspace();
    if idle(Path::new(&w.path)).is_err() {
        assert!(!f.store.inspect(&w.id).unwrap().collectable);
        return;
    }
    let mut child = Command::new("sleep")
        .arg("30")
        .current_dir(&w.checkouts[0].path)
        .spawn()
        .unwrap();
    let observed = f.store.inspect(&w.id).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(!observed.collectable);
    assert!(observed
        .retention_reasons
        .iter()
        .any(|r| r.contains("active processes")));
    f.ok(
        "collect",
        Action::Collect(CollectWorkspace {
            workspace_id: w.id,
            expected_revision: w.revision,
        }),
    );
    assert!(!Path::new(&w.checkouts[0].path).exists());
}

#[test]
fn integration_evidence_requires_actual_ancestry_and_retention() {
    let f = Fixture::new();
    f.ok("provision", f.provision());
    let w = f.workspace();
    f.ok(
        "lease",
        Action::Lease(LeaseWorkspace {
            workspace_id: w.id.clone(),
            owner: "test".into(),
            expected_revision: w.revision,
            ttl_seconds: 60,
            ..Default::default()
        }),
    );
    let path = Path::new(&w.checkouts[0].path);
    fs::write(path.join("README"), "feature\n").unwrap();
    git(path, &["commit", "-am", "feature"]).unwrap();
    let w = f.workspace();
    let release = Action::Release(ReleaseWorkspace {
        workspace_id: w.id.clone(),
        owner: "test".into(),
        expected_revision: w.revision,
        generation: 1,
        integrated_into: vec![RepositoryInput {
            repository_id: "desktop".into(),
            base_revision: "main".into(),
        }],
        ..Default::default()
    });
    assert_eq!(
        f.apply("release", release.clone()).phase,
        OperationPhase::Blocked as i32
    );
    git(&f.repo, &["merge", "--ff-only", &w.checkouts[0].branch]).unwrap();
    f.ok("release", release);
    assert_eq!(
        f.workspace().integration[0].source_commit,
        commit(path, "HEAD").unwrap()
    );
    let mut s = f.store.snapshot().unwrap();
    s.workspaces[0].collect_after = now() + 3600;
    f.store.save(&mut s).unwrap();
    assert!(f
        .store
        .inspect(&w.id)
        .unwrap()
        .retention_reasons
        .iter()
        .any(|r| r.contains("retained until")));
}

#[test]
fn legacy_removal_cannot_bypass_workspace_ownership() {
    let f = Fixture::new();
    f.ok("provision", f.provision());
    assert!(f
        .store
        .ensure_unmanaged(Path::new(&f.workspace().checkouts[0].path))
        .is_err());
    assert!(f.store.ensure_unmanaged(&f.repo).is_ok());
}
