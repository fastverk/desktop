//! Local workspace service. Blocking Git/filesystem work stays off the reactor.
use fvkit::workspace::Store;
use fvkit::workspace_proto::{workspace_service_server::WorkspaceService, *};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

#[derive(Clone, Default)]
pub struct Service {
    pub store: Option<Store>,
}
impl Service {
    fn store(&self) -> Result<Store, Status> {
        self.store
            .clone()
            .map_or_else(|| Store::configured().map_err(failure), Ok)
    }
}
fn failure(e: impl std::fmt::Display) -> Status {
    Status::failed_precondition(e.to_string())
}
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<Response<T>, Status> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(failure)?
        .map(Response::new)
        .map_err(failure)
}
#[tonic::async_trait]
impl WorkspaceService for Service {
    async fn snapshot(
        &self,
        _: Request<SnapshotRequest>,
    ) -> Result<Response<WorkspaceSnapshot>, Status> {
        let store = self.store()?;
        blocking(move || store.snapshot()).await
    }
    type WatchStream = ReceiverStream<Result<WorkspaceSnapshot, Status>>;
    async fn watch(
        &self,
        request: Request<SnapshotRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let store = self.store()?;
        let mut after = request.into_inner().after_revision;
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let mut first = true;
            loop {
                let s = store.clone();
                let next = tokio::task::spawn_blocking(move || s.snapshot()).await;
                match next {
                    Ok(Ok(snapshot)) => {
                        // A reset/replaced journal also forces a resnapshot.
                        if first || snapshot.revision != after {
                            after = snapshot.revision;
                            first = false;
                            if sender.send(Ok(snapshot)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        let _ = sender.send(Err(failure(e))).await;
                        break;
                    }
                    Err(e) => {
                        let _ = sender.send(Err(failure(e))).await;
                        break;
                    }
                }
                tokio::select! {
                    _ = sender.closed() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {},
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(receiver)))
    }
    async fn apply(
        &self,
        request: Request<WorkspaceCommand>,
    ) -> Result<Response<WorkspaceOperation>, Status> {
        let store = self.store()?;
        blocking(move || store.apply(request.into_inner())).await
    }
    async fn inspections(
        &self,
        _: Request<SnapshotRequest>,
    ) -> Result<Response<WorkspaceInspections>, Status> {
        let store = self.store()?;
        blocking(move || store.inspections()).await
    }
    async fn inspect(
        &self,
        request: Request<InspectRequest>,
    ) -> Result<Response<WorkspaceInspection>, Status> {
        let store = self.store()?;
        blocking(move || store.inspect(&request.into_inner().workspace_id)).await
    }
    async fn resolve_build_context(
        &self,
        request: Request<InspectRequest>,
    ) -> Result<Response<BuildContext>, Status> {
        let store = self.store()?;
        blocking(move || store.build_context(&request.into_inner().workspace_id)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fvkit::workspace_proto::{
        workspace_command::Action, workspace_service_client::WorkspaceServiceClient,
    };
    #[tokio::test]
    async fn generated_client_mutates_watches_and_reconnects_through_gateway() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("fvd-workspace-rpc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.clone(), dir.join("state"), dir.join("cache"));
        let socket = dir.join("fvd.sock");
        let incoming = fvkit::ipc::bind(&socket).unwrap();
        let routes = crate::server::gateway_with_workspace(
            std::sync::Arc::new(crate::plugins::Registry::default()),
            Service {
                store: Some(store.clone()),
            },
        );
        let server = tokio::spawn(
            tonic::transport::Server::builder()
                .add_routes(routes)
                .serve_with_incoming(incoming),
        );
        let channel = fvkit::ipc::connect_channel(&socket).await.unwrap();
        let mut client = WorkspaceServiceClient::new(channel);
        let before = client
            .snapshot(SnapshotRequest::default())
            .await
            .unwrap()
            .into_inner();
        assert!(before.projects.is_empty());
        let mut watch = client
            .watch(SnapshotRequest {
                after_revision: before.revision,
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            watch.message().await.unwrap().unwrap().revision,
            before.revision
        );
        let command = WorkspaceCommand {
            request_id: "project-rpc".into(),
            action: Some(Action::PutProject(PutProject {
                project: Some(ProjectSpec {
                    id: "research".into(),
                    ..Default::default()
                }),
                expected_revision: 0,
            })),
        };
        let op = client.apply(command.clone()).await.unwrap().into_inner();
        assert_eq!(op.phase, OperationPhase::Succeeded as i32, "{}", op.detail);
        assert_eq!(client.apply(command).await.unwrap().into_inner(), op);
        let changed = tokio::time::timeout(std::time::Duration::from_secs(5), watch.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(changed.revision > before.revision);
        assert_eq!(changed.projects[0].id, "research");
        assert_eq!(store.snapshot().unwrap().operations[0].id, "project-rpc");
        let mut resumed = client
            .watch(SnapshotRequest {
                after_revision: changed.revision,
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            resumed.message().await.unwrap().unwrap().revision,
            changed.revision
        );
        server.abort();
        let _ = server.await;
        drop(watch);
        drop(resumed);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
