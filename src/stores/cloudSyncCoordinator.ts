type EntityRefreshHandler = () => Promise<void>;

let entityRefreshHandler: EntityRefreshHandler | null = null;

export function registerCloudSyncEntityRefresh(handler: EntityRefreshHandler): () => void {
  entityRefreshHandler = handler;
  return () => {
    if (entityRefreshHandler === handler) {
      entityRefreshHandler = null;
    }
  };
}

export async function refreshCloudSyncEntities(): Promise<void> {
  await entityRefreshHandler?.();
}
