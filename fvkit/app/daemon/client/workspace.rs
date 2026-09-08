//! Generated-client CLI for fvd's local workspace service. Machine interchange
//! is protobuf (`apply COMMAND.pb`, `--protobuf`); default output is for humans.
use anyhow::{bail, Context, Result};
use fvkit::workspace_proto::{
    workspace_command::Action, workspace_service_client::WorkspaceServiceClient, *,
};
use prost::Message;
use std::io::Write;

fn arg(args: &[String], i: usize) -> Result<String> {
    args.get(i)
        .cloned()
        .with_context(|| format!("missing argument {i}; run fv-workspace help"))
}
fn number(args: &[String], i: usize) -> Result<u64> {
    Ok(arg(args, i)?.parse()?)
}
fn inputs(args: &[String], start: usize) -> Result<Vec<RepositoryInput>> {
    args.iter()
        .skip(start)
        .map(|v| {
            let (id, revision) = v.split_once('=').context("expected REPO=REVISION")?;
            Ok(RepositoryInput {
                repository_id: id.into(),
                base_revision: revision.into(),
            })
        })
        .collect()
}
fn output<T: Message + std::fmt::Debug>(value: T, binary: bool) -> Result<()> {
    if binary {
        std::io::stdout().write_all(&value.encode_to_vec())?;
    } else {
        println!("{value:#?}");
    }
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let binary = args.last().is_some_and(|a| a == "--protobuf");
    if binary {
        args.pop();
    }
    if args.is_empty() || args[0] == "help" || args[0] == "--help" {
        println!(
            "fv-workspace: all commands call fvd over FASTVERK_SOCKET\n\
  snapshot | watch [REVISION] | inspect WORKSPACE | context WORKSPACE\n\
  apply COMMAND.pb\n\
  register REQUEST ID HOST NAMESPACE NAME EXISTING_CLONE\n\
  clone REQUEST ID HOST NAMESPACE NAME CLONE_URL\n\
  project REQUEST ID EXPECTED_REVISION RETENTION_SECONDS [REPO ...]\n\
  provision REQUEST WORKSPACE PROJECT [REPO=BASE_REVISION ...]\n\
  lease REQUEST WORKSPACE OWNER EXPECTED_REVISION GENERATION TTL_SECONDS\n\
  abandon REQUEST WORKSPACE OWNER EXPECTED_REVISION GENERATION REASON\n\
  release REQUEST WORKSPACE OWNER EXPECTED_REVISION GENERATION [REPO=INTEGRATED_REF ...]\n\
  collect REQUEST WORKSPACE EXPECTED_REVISION\n\
Append --protobuf for an encoded response. Watch prints human-readable snapshots.\n\
REQUEST is a caller-chosen idempotency key; retry the identical command to resume.\n\
No command moves existing repositories, force-removes content, or pushes Git refs."
        );
        return Ok(());
    }
    let channel = fvkit::ipc::connect_channel(&fvkit::paths::socket_path()?).await?;
    let mut client = WorkspaceServiceClient::new(channel);
    match args[0].as_str() {
        "snapshot" => {
            return output(
                client
                    .snapshot(SnapshotRequest::default())
                    .await?
                    .into_inner(),
                binary,
            )
        }
        "inspect" => {
            return output(
                client
                    .inspect(InspectRequest {
                        workspace_id: arg(&args, 1)?,
                    })
                    .await?
                    .into_inner(),
                binary,
            )
        }
        "context" => {
            return output(
                client
                    .resolve_build_context(InspectRequest {
                        workspace_id: arg(&args, 1)?,
                    })
                    .await?
                    .into_inner(),
                binary,
            )
        }
        "watch" => {
            if binary {
                bail!("watch is a snapshot stream; use the generated gRPC client for machine consumption");
            }
            let mut stream = client
                .watch(SnapshotRequest {
                    after_revision: args.get(1).map_or(Ok(0), |s| s.parse())?,
                })
                .await?
                .into_inner();
            while let Some(s) = stream.message().await? {
                output(s, false)?;
            }
            return Ok(());
        }
        _ => {}
    }
    let command = if args[0] == "apply" {
        WorkspaceCommand::decode(std::fs::read(arg(&args, 1)?)?.as_slice())?
    } else {
        let action = match args[0].as_str() {
            "clone" => Action::RegisterRepository(RegisterRepository {
                repository: Some(Repository {
                    id: arg(&args, 2)?,
                    host: arg(&args, 3)?,
                    namespace: arg(&args, 4)?,
                    name: arg(&args, 5)?,
                    clone_url: arg(&args, 6)?,
                    ..Default::default()
                }),
            }),
            "register" => Action::RegisterRepository(RegisterRepository {
                repository: Some(Repository {
                    id: arg(&args, 2)?,
                    host: arg(&args, 3)?,
                    namespace: arg(&args, 4)?,
                    name: arg(&args, 5)?,
                    path: arg(&args, 6)?,
                    ..Default::default()
                }),
            }),
            "project" => Action::PutProject(PutProject {
                expected_revision: number(&args, 3)?,
                project: Some(ProjectSpec {
                    id: arg(&args, 2)?,
                    display_name: arg(&args, 2)?,
                    retention_seconds: number(&args, 4)?,
                    repository_ids: args.iter().skip(5).cloned().collect(),
                    ..Default::default()
                }),
            }),
            "provision" => Action::Provision(ProvisionWorkspace {
                id: arg(&args, 2)?,
                display_name: arg(&args, 2)?,
                project_id: arg(&args, 3)?,
                inputs: inputs(&args, 4)?,
            }),
            "lease" => Action::Lease(LeaseWorkspace {
                workspace_id: arg(&args, 2)?,
                owner: arg(&args, 3)?,
                expected_revision: number(&args, 4)?,
                generation: number(&args, 5)?,
                ttl_seconds: number(&args, 6)?,
            }),
            "abandon" | "release" => Action::Release(ReleaseWorkspace {
                workspace_id: arg(&args, 2)?,
                owner: arg(&args, 3)?,
                expected_revision: number(&args, 4)?,
                generation: number(&args, 5)?,
                abandonment_reason: if args[0] == "abandon" {
                    arg(&args, 6)?
                } else {
                    String::new()
                },
                integrated_into: if args[0] == "release" {
                    inputs(&args, 6)?
                } else {
                    vec![]
                },
            }),
            "collect" => Action::Collect(CollectWorkspace {
                workspace_id: arg(&args, 2)?,
                expected_revision: number(&args, 3)?,
            }),
            other => bail!("unknown command {other}; run fv-workspace help"),
        };
        WorkspaceCommand {
            request_id: arg(&args, 1)?,
            action: Some(action),
        }
    };
    let operation = client.apply(command).await?.into_inner();
    let success = operation.phase == OperationPhase::Succeeded as i32;
    output(operation, binary)?;
    if !success {
        bail!("operation blocked; inspect detail above and retry the identical request after resolving it");
    }
    Ok(())
}
