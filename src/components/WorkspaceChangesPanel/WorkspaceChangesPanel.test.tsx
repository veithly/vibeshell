import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WorkspaceChangesPanel } from './WorkspaceChangesPanel';

const { safeInvokeMock } = vi.hoisted(() => ({
  safeInvokeMock: vi.fn(),
}));

vi.mock('../../lib/tauri', () => ({
  safeInvoke: safeInvokeMock,
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

const status = {
  root: '/workspace',
  branch: 'main',
  files: [
    {
      path: 'src/example.ts',
      oldPath: null,
      kind: 'modified',
      staged: true,
      unstaged: true,
    },
  ],
};

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe('WorkspaceChangesPanel', () => {
  beforeEach(() => {
    safeInvokeMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('shows a diff IPC error instead of reporting that no text diff exists', async () => {
    safeInvokeMock.mockImplementation(async (command: string) => {
      if (command === 'coding_agent_workspace_status') {
        return { success: true, data: status };
      }
      return { success: false, error: { message: 'Git diff failed' } };
    });

    render(
      <WorkspaceChangesPanel
        open
        cwd="/workspace"
        sessionName="Codex · workspace"
        onClose={vi.fn()}
      />
    );

    expect(await screen.findByText('Git diff failed')).toBeInTheDocument();
    expect(screen.queryByText('workspaceChanges.noTextDiff')).not.toBeInTheDocument();
  });

  it('announces file kind and both index states', async () => {
    safeInvokeMock.mockImplementation(async (command: string) => {
      if (command === 'coding_agent_workspace_status') {
        return { success: true, data: status };
      }
      return {
        success: true,
        data: { path: status.files[0].path, oldPath: null, content: '', truncated: false },
      };
    });

    render(
      <WorkspaceChangesPanel open cwd="/workspace" onClose={vi.fn()} />
    );

    const file = await screen.findByRole('button', {
      name: /example\.ts.*src.*workspaceChanges\.fileKinds\.modified.*workspaceChanges\.staged.*workspaceChanges\.unstaged/,
    });
    expect(file).toHaveAttribute('aria-current', 'true');
  });

  it('shows the no-text-diff state for a binary diff without hunks', async () => {
    safeInvokeMock.mockImplementation(async (command: string) => {
      if (command === 'coding_agent_workspace_status') {
        return { success: true, data: status };
      }
      return {
        success: true,
        data: {
          path: status.files[0].path,
          oldPath: null,
          content: [
            'diff --git a/image.png b/image.png',
            'index 1111111..2222222 100644',
            'Binary files a/image.png and b/image.png differ',
            '',
          ].join('\n'),
          truncated: false,
        },
      };
    });

    render(
      <WorkspaceChangesPanel open cwd="/workspace" onClose={vi.fn()} />
    );

    expect(await screen.findByText('workspaceChanges.noTextDiff')).toBeInTheDocument();
  });

  it('waits for a pending status request before scheduling the next poll', async () => {
    vi.useFakeTimers();
    const firstStatus = deferred<{ success: true; data: typeof status }>();
    let statusCalls = 0;
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'coding_agent_workspace_status') {
        statusCalls += 1;
        if (statusCalls === 1) return firstStatus.promise;
        return Promise.resolve({ success: true, data: status });
      }
      return Promise.resolve({
        success: true,
        data: { path: status.files[0].path, oldPath: null, content: '', truncated: false },
      });
    });

    render(
      <WorkspaceChangesPanel open cwd="/workspace" onClose={vi.fn()} />
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(statusCalls).toBe(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(7_500);
    });
    expect(statusCalls).toBe(1);

    await act(async () => {
      firstStatus.resolve({ success: true, data: status });
      await firstStatus.promise;
      await Promise.resolve();
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_499);
    });
    expect(statusCalls).toBe(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(statusCalls).toBe(2);
  });

  it('refreshes the selected diff during the quiet status poll', async () => {
    vi.useFakeTimers();
    let diffCalls = 0;
    safeInvokeMock.mockImplementation(async (command: string) => {
      if (command === 'coding_agent_workspace_status') {
        return { success: true, data: status };
      }
      diffCalls += 1;
      return {
        success: true,
        data: { path: status.files[0].path, oldPath: null, content: '', truncated: false },
      };
    });

    render(
      <WorkspaceChangesPanel open cwd="/workspace" onClose={vi.fn()} />
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(diffCalls).toBe(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2500);
      await Promise.resolve();
    });

    expect(diffCalls).toBe(2);
  });
});
