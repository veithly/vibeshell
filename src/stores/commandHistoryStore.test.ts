import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useCommandHistoryStore } from './commandHistoryStore';

const safeInvokeMock = vi.hoisted(() => vi.fn());

vi.mock('../lib/tauri', () => ({
  safeInvoke: safeInvokeMock,
}));

const entry = {
  id: 'history-1',
  server_id: 'server-a',
  command: 'systemctl status nginx',
  is_favorite: false,
  use_count: 1,
  last_used_at: 20,
  created_at: 10,
};

describe('commandHistoryStore', () => {
  beforeEach(() => {
    safeInvokeMock.mockReset();
    useCommandHistoryStore.setState({
      entries: [],
      activeServerId: null,
      loading: false,
      error: null,
    });
  });

  it('loads history with the selected server and search filters', async () => {
    safeInvokeMock.mockResolvedValue({ success: true, data: [entry] });

    await useCommandHistoryStore
      .getState()
      .fetchHistory('server-a', 'nginx', true);

    expect(safeInvokeMock).toHaveBeenCalledWith('history_list', {
      input: {
        serverId: 'server-a',
        query: 'nginx',
        favoritesOnly: true,
        limit: 200,
      },
    });
    expect(useCommandHistoryStore.getState().entries).toEqual([entry]);
  });

  it('records, favorites, and deletes a command without crossing server scope', async () => {
    useCommandHistoryStore.setState({ activeServerId: 'server-a' });
    safeInvokeMock
      .mockResolvedValueOnce({ success: true, data: entry })
      .mockResolvedValueOnce({ success: true, data: undefined })
      .mockResolvedValueOnce({ success: true, data: undefined });

    await useCommandHistoryStore.getState().recordCommand('server-a', entry.command);
    expect(safeInvokeMock).toHaveBeenNthCalledWith(1, 'history_record', {
      input: { serverId: 'server-a', command: entry.command },
    });

    await useCommandHistoryStore.getState().setFavorite(entry.id, true);
    expect(useCommandHistoryStore.getState().entries[0].is_favorite).toBe(true);
    expect(safeInvokeMock).toHaveBeenNthCalledWith(2, 'history_set_favorite', {
      input: { id: entry.id, isFavorite: true },
    });

    await useCommandHistoryStore.getState().deleteEntry(entry.id);
    expect(useCommandHistoryStore.getState().entries).toHaveLength(0);
    expect(safeInvokeMock).toHaveBeenNthCalledWith(3, 'history_delete', { id: entry.id });
  });
});
