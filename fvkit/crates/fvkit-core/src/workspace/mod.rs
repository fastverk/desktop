//! Protobuf-backed workspace lifecycle. The journal is authoritative; Git and
//! filesystem projections are reconciled when an interrupted command is retried.
use crate::workspace_proto::{workspace_command::Action, *};
use anyhow::{Context, Result};
use prost::Message;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

// Explicit unlock avoids retaining an inherited flock until a concurrently
// spawned child finishes exec on Unix. Closing only the parent fd is insufficient.
struct WriterLock(File);
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
    state: PathBuf,
    output_root: PathBuf,
}
impl Store {
    pub fn configured() -> Result<Self> {
        let c = crate::config::Config::load()?;
        Ok(Self::new(
            c.repos_root,
            crate::paths::config_dir()?.join("workspaces"),
            c.output_user_root,
        ))
    }
    pub fn new(root: PathBuf, state: PathBuf, output_root: PathBuf) -> Self {
        Self {
            root,
            state,
            output_root,
        }
    }
    /// Read-only, including on a missing volume. Never creates directories.
    pub fn snapshot(&self) -> Result<WorkspaceSnapshot> {
        match fs::read(self.state.join("catalog.pb")) {
            Ok(b) => {
                let s =
                    WorkspaceSnapshot::decode(b.as_slice()).context("decode workspace journal")?;
                anyhow::ensure!(
                    s.schema_version == 1,
                    "unsupported journal version {}",
                    s.schema_version
                );
                anyhow::ensure!(
                    Path::new(&s.root) == self.root,
                    "workspace root changed; explicit migration required"
                );
                Ok(s)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WorkspaceSnapshot {
                schema_version: 1,
                root: text(&self.root)?,
                ..Default::default()
            }),
            Err(e) => Err(e).context("read workspace journal"),
        }
    }
    fn save(&self, s: &mut WorkspaceSnapshot) -> Result<()> {
        s.revision = s.revision.checked_add(1).context("revision exhausted")?;
        atomic_write(&self.state.join("catalog.pb"), &s.encode_to_vec())
    }
    /// Lock across processes, not just RPC tasks. A blocked operation is durable;
    /// retry the identical request after resolving its reported condition.
    pub fn apply(&self, command: WorkspaceCommand) -> Result<WorkspaceOperation> {
        token(&command.request_id)?;
        anyhow::ensure!(command.action.is_some(), "action required");
        safe_path(&self.state)?;
        fs::create_dir_all(&self.state)?;
        let path = self.state.join("writer.lock");
        safe_path(&path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        lock.try_lock()
            .context("another lifecycle mutation is running; retry")?;
        let _lock = WriterLock(lock);
        let mut s = self.snapshot()?;
        let i = if let Some(i) = s.operations.iter().position(|o| o.id == command.request_id) {
            anyhow::ensure!(
                s.operations[i].command.as_ref() == Some(&command),
                "request_id reused with a different payload"
            );
            if s.operations[i].phase == OperationPhase::Succeeded as i32 {
                return Ok(s.operations[i].clone());
            }
            i
        } else {
            s.operations.push(WorkspaceOperation {
                id: command.request_id.clone(),
                command: Some(command.clone()),
                phase: OperationPhase::Running as i32,
                ..Default::default()
            });
            s.operations.len() - 1
        };
        s.operations[i].phase = OperationPhase::Running as i32;
        self.save(&mut s)?;
        let result = self.execute(&mut s, command.action.context("action required")?);
        s.operations[i].phase = if result.is_ok() {
            OperationPhase::Succeeded
        } else {
            OperationPhase::Blocked
        } as i32;
        s.operations[i].detail = result
            .err()
            .map_or_else(|| "complete".into(), |e| format!("{e:#}"));
        s.operations[i].revision = s.revision + 1;
        self.save(&mut s)?;
        Ok(s.operations[i].clone())
    }
    fn execute(&self, s: &mut WorkspaceSnapshot, action: Action) -> Result<()> {
        match action {
            Action::RegisterRepository(r) => self.register(s, r),
            Action::PutProject(p) => self.project(s, p),
            Action::Provision(p) => self.provision(s, p),
            Action::Lease(l) => self.lease(s, l),
            Action::Release(r) => self.release(s, r),
            Action::Collect(c) => self.collect(s, c),
        }
    }
    fn register(&self, s: &mut WorkspaceSnapshot, request: RegisterRepository) -> Result<()> {
        let mut r = request.repository.context("repository required")?;
        token(&r.id)?;
        token(&r.host)?;
        token(&r.name)?;
        namespace(&r.namespace)?;
        let cloning = r.path.is_empty();
        let previous = s.repositories.iter().position(|x| x.id == r.id);
        if cloning {
            let https = format!("https://{}/{}/{}.git", r.host, r.namespace, r.name);
            let ssh = format!("git@{}:{}/{}.git", r.host, r.namespace, r.name);
            anyhow::ensure!(r.clone_url == https || r.clone_url == ssh, "clone_url must match the explicit identity (canonical HTTPS or SSH, no embedded credentials)");
            self.prepare_root()?;
            r.path = text(
                &self
                    .root
                    .join("repos")
                    .join(&r.host)
                    .join(&r.namespace)
                    .join(&r.name),
            )?;
        } else {
            r.clone_url.clear();
        }
        let path = Path::new(&r.path);
        safe_path(path)?;
        safe_path(&path.join(".git"))?;
        if let Some(i) = previous {
            let old = &s.repositories[i];
            anyhow::ensure!(
                cloning
                    && !old.ready
                    && old.path == r.path
                    && old.host == r.host
                    && old.namespace == r.namespace
                    && old.name == r.name
                    && old.clone_url == r.clone_url,
                "repository ID already registered"
            );
        } else {
            anyhow::ensure!(
                !s.repositories
                    .iter()
                    .any(|x| x.host.eq_ignore_ascii_case(&r.host)
                        && x.namespace == r.namespace
                        && x.name == r.name),
                "repository identity already registered"
            );
            anyhow::ensure!(
                !s.repositories.iter().any(|x| Path::new(&x.path) == path),
                "physical repository already registered"
            );
            if cloning {
                anyhow::ensure!(
                    !path.exists(),
                    "clone destination already exists; adopt it explicitly"
                );
                r.ready = false;
                s.repositories.push(r.clone());
                self.save(s)?;
            }
        }
        if cloning && !path.exists() {
            fs::create_dir_all(path.parent().context("clone parent")?)?;
            git(&self.root, &["clone", "--", &r.clone_url, &r.path])?;
        }
        anyhow::ensure!(
            path.join(".git").is_dir(),
            "an ordinary primary clone is required; partial clones are retained for inspection"
        );
        let canonical = path.canonicalize()?;
        anyhow::ensure!(
            git(path, &["rev-parse", "--show-toplevel"])? == text(&canonical)?,
            "path is not repository root"
        );
        if cloning {
            anyhow::ensure!(
                git(path, &["config", "--get", "remote.origin.url"])? == r.clone_url,
                "partial clone origin changed"
            );
            commit(path, "HEAD").context("clone has no HEAD; inspect it before retrying")?;
        }
        r.path = text(&canonical)?;
        r.ready = true;
        if let Some(i) = s.repositories.iter().position(|x| x.id == r.id) {
            s.repositories[i] = r;
        } else {
            s.repositories.push(r);
        }
        Ok(())
    }
    fn project(&self, s: &mut WorkspaceSnapshot, request: PutProject) -> Result<()> {
        let mut p = request.project.context("project required")?;
        token(&p.id)?;
        unique(&p.repository_ids)?;
        anyhow::ensure!(
            p.retention_seconds <= 31_536_000,
            "retention exceeds one year"
        );
        for id in &p.repository_ids {
            repo(s, id)?;
        }
        let old = s.projects.iter().position(|x| x.id == p.id);
        anyhow::ensure!(
            old.map_or(0, |i| s.projects[i].revision) == request.expected_revision,
            "project revision changed"
        );
        p.revision = request
            .expected_revision
            .checked_add(1)
            .context("revision exhausted")?;
        self.prepare_root()?;
        atomic_write(
            &self.root.join("projects").join(&p.id).join("project.pb"),
            &p.encode_to_vec(),
        )?;
        if let Some(i) = old {
            s.projects[i] = p;
        } else {
            s.projects.push(p);
        }
        Ok(())
    }
    fn prepare_root(&self) -> Result<()> {
        safe_path(&self.root)?;
        anyhow::ensure!(
            self.root.is_dir(),
            "workspace volume is missing; mount it before provisioning"
        );
        for name in ["projects", "repos", "workspaces", "artifacts"] {
            let path = self.root.join(name);
            safe_path(&path)?;
            fs::create_dir_all(path)?;
        }
        Ok(())
    }
    fn provision(&self, s: &mut WorkspaceSnapshot, p: ProvisionWorkspace) -> Result<()> {
        token(&p.id)?;
        let i = if let Some(i) = s.workspaces.iter().position(|w| w.id == p.id) {
            let original = s
                .operations
                .iter()
                .filter_map(|o| o.command.as_ref())
                .filter_map(|c| match &c.action {
                    Some(Action::Provision(x)) => Some(x),
                    _ => None,
                })
                .find(|x| x.id == p.id)
                .context("missing provisioning journal")?;
            anyhow::ensure!(
                original == &p && s.workspaces[i].phase == WorkspacePhase::Provisioning as i32,
                "workspace ID already exists"
            );
            i
        } else {
            let project = s
                .projects
                .iter()
                .find(|x| x.id == p.project_id)
                .context("unknown project")?
                .clone();
            unique(
                &p.inputs
                    .iter()
                    .map(|x| x.repository_id.clone())
                    .collect::<Vec<_>>(),
            )?;
            self.prepare_root()?;
            let path = self.root.join("workspaces").join(&p.id);
            safe_path(&path)?;
            anyhow::ensure!(
                !path.exists(),
                "workspace path exists; refusing unknown contents"
            );
            let mut checkouts = Vec::new();
            for input in &p.inputs {
                anyhow::ensure!(
                    project.repository_ids.contains(&input.repository_id),
                    "repository is not a project member"
                );
                let r = repo(s, &input.repository_id)?;
                safe_path(Path::new(&r.path))?;
                let base = commit(Path::new(&r.path), &input.base_revision)?;
                let dest = path
                    .join("src")
                    .join(&r.host)
                    .join(&r.namespace)
                    .join(&r.name);
                safe_path(&dest)?;
                // Branches are reserved before side effects. Existing branches
                // are never silently repurposed for a newly created workspace.
                let branch = format!("fv/{}/{}", p.id, r.id);
                anyhow::ensure!(
                    commit(Path::new(&r.path), &format!("refs/heads/{branch}")).is_err(),
                    "workspace branch already exists"
                );
                checkouts.push(WorkspaceCheckout {
                    repository_id: r.id.clone(),
                    path: text(&dest)?,
                    base_commit: base,
                    branch,
                    removed: false,
                    removal_started: false,
                });
            }
            s.workspaces.push(ManagedWorkspace {
                id: p.id.clone(),
                project_id: project.id.clone(),
                display_name: p.display_name,
                path: text(&path)?,
                project: Some(project),
                checkouts,
                phase: WorkspacePhase::Provisioning as i32,
                revision: 1,
                created_at: now(),
                ..Default::default()
            });
            self.save(s)?; // Explicit pins and paths are durable before Git runs.
            s.workspaces.len() - 1
        };
        let w = s.workspaces[i].clone();
        safe_path(Path::new(&w.path))?;
        for part in ["src", "run", "out"] {
            let p = Path::new(&w.path).join(part);
            safe_path(&p)?;
            fs::create_dir_all(p)?;
        }
        for checkout in &w.checkouts {
            let r = repo(s, &checkout.repository_id)?;
            let source = Path::new(&r.path);
            let dest = Path::new(&checkout.path);
            safe_path(source)?;
            safe_path(dest)?;
            if dest.exists() {
                verify_checkout(r, checkout)?;
                anyhow::ensure!(
                    commit(dest, "HEAD")? == checkout.base_commit,
                    "interrupted checkout has advanced; inspect it"
                );
                anyhow::ensure!(
                    git(
                        dest,
                        &[
                            "status",
                            "--porcelain",
                            "--untracked-files=all",
                            "--ignored"
                        ]
                    )?
                    .is_empty(),
                    "interrupted checkout contains local files; inspect it"
                );
            } else {
                fs::create_dir_all(dest.parent().context("checkout parent")?)?;
                if let Ok(existing) = commit(source, &format!("refs/heads/{}", checkout.branch)) {
                    anyhow::ensure!(
                        existing == checkout.base_commit,
                        "reserved branch has advanced"
                    );
                    git(
                        source,
                        &["worktree", "add", &checkout.path, &checkout.branch],
                    )?;
                } else {
                    git(
                        source,
                        &[
                            "worktree",
                            "add",
                            "-b",
                            &checkout.branch,
                            &checkout.path,
                            &checkout.base_commit,
                        ],
                    )?;
                }
            }
        }
        let mut ready = s.workspaces[i].clone();
        ready.phase = WorkspacePhase::Ready as i32;
        ready.revision += 1;
        self.project_workspace(&ready)?;
        s.workspaces[i] = ready;
        Ok(())
    }
    fn project_workspace(&self, w: &ManagedWorkspace) -> Result<()> {
        atomic_write(&Path::new(&w.path).join("workspace.pb"), &w.encode_to_vec())
    }
    fn lease(&self, s: &mut WorkspaceSnapshot, l: LeaseWorkspace) -> Result<()> {
        token(&l.owner)?;
        anyhow::ensure!(
            (1..=3600).contains(&l.ttl_seconds),
            "lease TTL must be 1..3600 seconds"
        );
        let w = workspace_mut(s, &l.workspace_id)?;
        anyhow::ensure!(
            w.revision == l.expected_revision,
            "workspace revision changed"
        );
        anyhow::ensure!(
            [WorkspacePhase::Ready as i32, WorkspacePhase::Active as i32].contains(&w.phase),
            "workspace is not ready"
        );
        let old = w.lease.clone().unwrap_or_default();
        anyhow::ensure!(old.generation == l.generation, "stale fencing generation");
        anyhow::ensure!(
            old.owner == l.owner || old.expires_at <= now(),
            "workspace has a live owner"
        );
        let generation = if old.owner == l.owner && old.expires_at > now() {
            old.generation
        } else {
            old.generation
                .checked_add(1)
                .context("generation exhausted")?
        };
        w.lease = Some(WorkspaceLease {
            owner: l.owner,
            generation,
            expires_at: now() + i64::try_from(l.ttl_seconds)?,
        });
        w.phase = WorkspacePhase::Active as i32;
        w.revision += 1;
        Ok(())
    }
    fn release(&self, s: &mut WorkspaceSnapshot, r: ReleaseWorkspace) -> Result<()> {
        let w = s
            .workspaces
            .iter()
            .find(|w| w.id == r.workspace_id)
            .context("unknown workspace")?
            .clone();
        anyhow::ensure!(
            w.revision == r.expected_revision,
            "workspace revision changed"
        );
        anyhow::ensure!(
            w.phase == WorkspacePhase::Active as i32,
            "workspace must be actively leased before release"
        );
        let l = w.lease.as_ref().context("workspace has no owner")?;
        anyhow::ensure!(
            l.owner == r.owner && l.generation == r.generation && l.expires_at > now(),
            "live owner and current fencing generation required"
        );
        anyhow::ensure!(
            r.abandonment_reason.trim().is_empty() || r.integrated_into.is_empty(),
            "choose integration or explicit abandonment"
        );
        let mut evidence = Vec::new();
        if r.abandonment_reason.trim().is_empty() {
            unique(
                &r.integrated_into
                    .iter()
                    .map(|x| x.repository_id.clone())
                    .collect::<Vec<_>>(),
            )?;
            anyhow::ensure!(
                r.integrated_into.len() == w.checkouts.len(),
                "integration target required for every checkout"
            );
            for c in &w.checkouts {
                let t = r
                    .integrated_into
                    .iter()
                    .find(|x| x.repository_id == c.repository_id)
                    .context("missing integration target")?;
                verify_checkout(repo(s, &c.repository_id)?, c)?;
                let source = commit(Path::new(&c.path), "HEAD")?;
                let target = commit(Path::new(&c.path), &t.base_revision)?;
                git(
                    Path::new(&c.path),
                    &["merge-base", "--is-ancestor", &source, &target],
                )
                .context("workspace commits are not in integration target")?;
                evidence.push(IntegrationEvidence {
                    repository_id: c.repository_id.clone(),
                    source_commit: source,
                    target_commit: target,
                });
            }
        }
        let w = workspace_mut(s, &r.workspace_id)?;
        w.phase = WorkspacePhase::Released as i32;
        w.released_at = now();
        w.collect_after = now()
            + i64::try_from(
                w.project
                    .as_ref()
                    .context("missing project")?
                    .retention_seconds,
            )?;
        w.abandonment_reason = r.abandonment_reason;
        w.integration = evidence;
        w.lease.as_mut().context("missing lease")?.expires_at = 0;
        w.revision += 1;
        Ok(())
    }
    /// Keep the legacy force-removal API from bypassing lifecycle ownership.
    pub fn ensure_unmanaged(&self, path: &Path) -> Result<()> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let resolved = absolute.canonicalize().unwrap_or(absolute);
        let s = self.snapshot()?;
        anyhow::ensure!(
            !s.workspaces.iter().any(|w| resolved.starts_with(&w.path)),
            "managed workspace: use WorkspaceService collection after releasing ownership"
        );
        Ok(())
    }
    pub fn inspections(&self) -> Result<WorkspaceInspections> {
        let s = self.snapshot()?;
        let inspections = s
            .workspaces
            .iter()
            .filter(|w| w.phase != WorkspacePhase::Removed as i32)
            .map(|w| self.inspect_snapshot(&s, &w.id))
            .collect::<Result<Vec<_>>>()?;
        Ok(WorkspaceInspections { inspections })
    }
    pub fn inspect(&self, id: &str) -> Result<WorkspaceInspection> {
        self.inspect_snapshot(&self.snapshot()?, id)
    }
    fn inspect_snapshot(&self, s: &WorkspaceSnapshot, id: &str) -> Result<WorkspaceInspection> {
        let w = s
            .workspaces
            .iter()
            .find(|w| w.id == id)
            .context("unknown workspace")?;
        let mut reasons = Vec::new();
        if ![
            WorkspacePhase::Released as i32,
            WorkspacePhase::Removing as i32,
        ]
        .contains(&w.phase)
        {
            reasons.push("workspace has not been explicitly released".into());
        }
        if w.lease.as_ref().is_some_and(|l| l.expires_at > now()) {
            reasons.push("live ownership lease".into());
        }
        if w.collect_after > now() {
            reasons.push(format!("retained until {}", w.collect_after));
        }
        if w.abandonment_reason.trim().is_empty() && w.integration.len() != w.checkouts.len() {
            reasons.push("missing integration or abandonment evidence".into());
        }
        let mut checkouts = Vec::new();
        for c in &w.checkouts {
            if c.removed {
                continue;
            }
            let mut o = CheckoutInspection {
                repository_id: c.repository_id.clone(),
                path: c.path.clone(),
                ..Default::default()
            };
            let result = (|| -> Result<()> {
                verify_checkout(repo(s, &c.repository_id)?, c)?;
                let p = Path::new(&c.path);
                o.head = commit(p, "HEAD")?;
                o.dirty = !git(p, &["status", "--porcelain", "--untracked-files=all"])?.is_empty();
                o.ignored_files = !git(
                    p,
                    &["ls-files", "--others", "--ignored", "--exclude-standard"],
                )?
                .is_empty();
                if o.dirty {
                    reasons.push(format!(
                        "{}: uncommitted or untracked content",
                        c.repository_id
                    ));
                }
                if o.ignored_files {
                    reasons.push(format!(
                        "{}: ignored content needs preservation",
                        c.repository_id
                    ));
                }
                if w.abandonment_reason.trim().is_empty()
                    && !w
                        .integration
                        .iter()
                        .any(|e| e.repository_id == c.repository_id && e.source_commit == o.head)
                {
                    reasons.push(format!(
                        "{}: current HEAD lacks integration evidence",
                        c.repository_id
                    ));
                }
                Ok(())
            })();
            if let Err(e) = result {
                o.problem = format!("{e:#}");
                reasons.push(format!("{}: {}", c.repository_id, o.problem));
            }
            checkouts.push(o);
        }
        if w.phase != WorkspacePhase::Removed as i32 {
            if let Err(e) = self.check_extra_content(w) {
                reasons.push(format!("{e:#}"));
            }
            if let Err(e) = idle(Path::new(&w.path)) {
                reasons.push(format!("{e:#}"));
            }
        }
        Ok(WorkspaceInspection {
            workspace: Some(w.clone()),
            checkouts,
            collectable: reasons.is_empty(),
            retention_reasons: reasons,
        })
    }
    /// Only empty scaffolding and the daemon's descriptor may sit outside Git.
    /// Logs/artifacts/unknown files are retained, never recursively removed.
    fn check_extra_content(&self, w: &ManagedWorkspace) -> Result<()> {
        fn walk(path: &Path, w: &ManagedWorkspace) -> Result<()> {
            safe_path(path)?;
            for e in fs::read_dir(path)? {
                let e = e?;
                let p = e.path();
                anyhow::ensure!(
                    !e.file_type()?.is_symlink(),
                    "unmanaged symlink retained: {}",
                    p.display()
                );
                if w.checkouts
                    .iter()
                    .any(|c| !c.removed && Path::new(&c.path) == p)
                {
                    continue;
                }
                if p == Path::new(&w.path).join("workspace.pb") {
                    anyhow::ensure!(e.file_type()?.is_file(), "projection is not a file");
                    let projected = ManagedWorkspace::decode(fs::read(&p)?.as_slice())?;
                    anyhow::ensure!(
                        projected.id == w.id
                            && projected.path == w.path
                            && projected.checkouts.len() == w.checkouts.len()
                            && projected.checkouts.iter().zip(&w.checkouts).all(|(a, b)| a
                                .repository_id
                                == b.repository_id
                                && a.path == b.path
                                && a.branch == b.branch
                                && a.base_commit == b.base_commit),
                        "modified workspace projection needs review"
                    );
                } else if e.file_type()?.is_dir() {
                    walk(&p, w)?;
                } else {
                    anyhow::bail!("unmanaged content retained: {}", p.display());
                }
            }
            Ok(())
        }
        walk(Path::new(&w.path), w)
    }
    fn collect(&self, s: &mut WorkspaceSnapshot, c: CollectWorkspace) -> Result<()> {
        let i = s
            .workspaces
            .iter()
            .position(|w| w.id == c.workspace_id)
            .context("unknown workspace")?;
        anyhow::ensure!(
            s.workspaces[i].revision == c.expected_revision,
            "workspace revision changed"
        );
        // Recover a crash between Git removal and the next journal replacement.
        // Only a checkout with durable removal intent can be reconciled this way.
        for j in 0..s.workspaces[i].checkouts.len() {
            let checkout = s.workspaces[i].checkouts[j].clone();
            if checkout.removal_started && !checkout.removed && !Path::new(&checkout.path).exists()
            {
                safe_path(Path::new(&checkout.path))?;
                let r = repo(s, &checkout.repository_id)?;
                safe_path(Path::new(&r.path))?;
                let listing = git(Path::new(&r.path), &["worktree", "list", "--porcelain"])?;
                if listing
                    .lines()
                    .any(|line| line == format!("worktree {}", checkout.path))
                {
                    git(Path::new(&r.path), &["worktree", "remove", &checkout.path])?;
                }
                s.workspaces[i].checkouts[j].removed = true;
                self.save(s)?;
            }
        }
        let inspection = self.inspect_snapshot(s, &c.workspace_id)?;
        anyhow::ensure!(
            inspection.collectable,
            "collection blocked: {}",
            inspection.retention_reasons.join("; ")
        );
        s.workspaces[i].phase = WorkspacePhase::Removing as i32;
        self.save(s)?;
        for j in 0..s.workspaces[i].checkouts.len() {
            let c = s.workspaces[i].checkouts[j].clone();
            if c.removed {
                continue;
            }
            // Recheck immediately before every removal, including ignored files
            // and external consumers that may have appeared since the preview.
            let check = self.inspect_snapshot(s, &s.workspaces[i].id)?;
            anyhow::ensure!(
                check.collectable,
                "collection blocked: {}",
                check.retention_reasons.join("; ")
            );
            s.workspaces[i].checkouts[j].removal_started = true;
            self.save(s)?;
            let r = repo(s, &c.repository_id)?;
            // Git rechecks dirty/locked state. Never force, delete branches, or
            // broadly prune registrations belonging to unrelated workspaces.
            git(Path::new(&r.path), &["worktree", "remove", &c.path])?;
            s.workspaces[i].checkouts[j].removed = true;
            self.project_workspace(&s.workspaces[i])?;
            self.save(s)?;
        }
        // Keep the tiny descriptor and empty scaffolding as a tombstone.
        let mut removed = s.workspaces[i].clone();
        removed.phase = WorkspacePhase::Removed as i32;
        removed.revision += 1;
        self.project_workspace(&removed)?;
        s.workspaces[i] = removed;
        Ok(())
    }
    pub fn build_context(&self, id: &str) -> Result<BuildContext> {
        let inspection = self.inspect(id)?;
        let w = inspection.workspace.context("workspace missing")?;
        anyhow::ensure!(
            [WorkspacePhase::Ready as i32, WorkspacePhase::Active as i32].contains(&w.phase),
            "workspace is not ready for builds"
        );
        let p = w.project.context("missing project")?;
        let mut sources = Vec::new();
        for c in inspection.checkouts {
            anyhow::ensure!(c.problem.is_empty(), "{}", c.problem);
            sources.push(BuildSource {
                repository_id: c.repository_id,
                checkout_path: c.path,
                commit: c.head,
                dirty: c.dirty || c.ignored_files,
            });
        }
        Ok(BuildContext {
            workspace_id: w.id.clone(),
            project_id: p.id.clone(),
            project_revision: p.revision,
            sources,
            validation_targets: p.validation_targets,
            output_base: text(&self.output_root.join("managed").join(&w.id))?,
            artifact_directory: text(&self.root.join("artifacts").join(&p.id).join(&w.id))?,
            requires_input_snapshot: true,
        })
    }
}
fn repo<'a>(s: &'a WorkspaceSnapshot, id: &str) -> Result<&'a Repository> {
    s.repositories
        .iter()
        .find(|r| r.id == id && r.ready)
        .context("unknown repository or clone not ready")
}
fn workspace_mut<'a>(s: &'a mut WorkspaceSnapshot, id: &str) -> Result<&'a mut ManagedWorkspace> {
    s.workspaces
        .iter_mut()
        .find(|w| w.id == id)
        .context("unknown workspace")
}
fn token(s: &str) -> Result<()> {
    anyhow::ensure!(
        !s.is_empty()
            && s.len() <= 100
            && s.as_bytes()[0].is_ascii_alphanumeric()
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)),
        "invalid ID/path component: {s}"
    );
    Ok(())
}
fn namespace(s: &str) -> Result<()> {
    anyhow::ensure!(!s.is_empty(), "namespace required");
    for part in s.split('/') {
        token(part)?;
    }
    Ok(())
}
fn unique(ids: &[String]) -> Result<()> {
    anyhow::ensure!(
        ids.iter().collect::<BTreeSet<_>>().len() == ids.len(),
        "duplicate repository IDs"
    );
    Ok(())
}
fn text(p: &Path) -> Result<String> {
    Ok(p.to_str().context("non-UTF-8 path")?.into())
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
fn safe_path(path: &Path) -> Result<()> {
    anyhow::ensure!(
        path.is_absolute(),
        "absolute path required: {}",
        path.display()
    );
    let mut prefix = PathBuf::new();
    for c in path.components() {
        anyhow::ensure!(
            matches!(c, Component::RootDir | Component::Normal(_)),
            "unsafe path: {}",
            path.display()
        );
        prefix.push(c);
        match fs::symlink_metadata(&prefix) {
            Ok(m) => anyhow::ensure!(
                !m.file_type().is_symlink(),
                "symlink in managed path: {}",
                prefix.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    safe_path(path)?;
    let parent = path.parent().context("missing parent")?;
    fs::create_dir_all(parent)?;
    let temp = path.with_extension("pb.tmp");
    safe_path(&temp)?;
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(temp, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
fn git(path: &Path, args: &[&str]) -> Result<String> {
    let o = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .context("run git")?;
    anyhow::ensure!(
        o.status.success(),
        "git {}: {}",
        args.first().unwrap_or(&""),
        String::from_utf8_lossy(&o.stderr).trim()
    );
    Ok(String::from_utf8(o.stdout)?.trim_end().into())
}
fn commit(path: &Path, revision: &str) -> Result<String> {
    anyhow::ensure!(!revision.is_empty(), "explicit revision required");
    git(
        path,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
    )
}
fn verify_checkout(r: &Repository, c: &WorkspaceCheckout) -> Result<()> {
    let path = Path::new(&c.path);
    safe_path(path)?;
    safe_path(&path.join(".git"))?;
    anyhow::ensure!(
        git(path, &["rev-parse", "--show-toplevel"])? == c.path,
        "checkout root changed"
    );
    safe_path(Path::new(&r.path))?;
    anyhow::ensure!(
        path.join(".git").is_file(),
        "linked checkout missing or replaced"
    );
    let common = git(
        path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    anyhow::ensure!(
        Path::new(&common).canonicalize()? == Path::new(&r.path).join(".git").canonicalize()?,
        "checkout belongs to a different Git repository"
    );
    anyhow::ensure!(
        git(path, &["symbolic-ref", "--short", "HEAD"])? == c.branch,
        "workspace branch changed; inspect it"
    );
    let registrations = git(Path::new(&r.path), &["worktree", "list", "--porcelain"])?;
    let first = format!("worktree {}", c.path);
    let block = registrations
        .split("\n\n")
        .find(|b| b.lines().next() == Some(first.as_str()))
        .context("checkout not registered")?;
    anyhow::ensure!(
        !block
            .lines()
            .any(|l| l == "locked" || l.starts_with("locked ")),
        "Git worktree is locked"
    );
    Ok(())
}
fn idle(path: &Path) -> Result<()> {
    safe_path(path)?;
    let o = Command::new("lsof")
        .args(["-t", "+D"])
        .arg(path)
        .output()
        .context("cannot check active consumers (lsof required)")?;
    anyhow::ensure!(
        o.stdout.is_empty(),
        "active processes hold files or working directories in the workspace"
    );
    anyhow::ensure!(
        o.status.code() == Some(1) && o.stderr.is_empty(),
        "process inspection inconclusive: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    Ok(())
}
#[cfg(test)]
mod tests;
