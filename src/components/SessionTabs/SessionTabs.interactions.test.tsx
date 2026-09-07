import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SessionTabs } from './SessionTabs';
import { useSessionStore, type Session } from '../../stores/sessionStore';
import { useFileWorkspaceStore } from '../../stores/fileWorkspaceStore';
import { usePluginWorkspaceStore } from '../../stores/pluginWorkspaceStore';
import { useDetachedOwnership } from '../../lib/detach';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../lib/tauri', () => ({ safeInvoke: vi.fn(async () => ({ success: true, data: null })) }));
const initial: Session = { id: 'local-test', serverId: 'zsh', serverName: 'Test shell', state: 'connected', sessionType: 'local', createdAt: 1 };
const kill = vi.fn(async (id: string) => { useSessionStore.getState().removeSession(id); return true; });

function releaseDrag(): void {
  const tab = screen.getByRole('tab', { name: /Test shell/ });
  fireEvent.mouseDown(tab, { button: 0, buttons: 1, clientX: 400, clientY: 60 });
  fireEvent.mouseMove(document, { button: 0, buttons: 1, clientX: 440, clientY: 65 });
  fireEvent.mouseUp(document, { button: 0, buttons: 0, clientX: 440, clientY: 65 });
}

describe('SessionTabs real button interactions', () => {
  beforeEach(() => {
    kill.mockClear();
    useSessionStore.setState({ sessions: [initial], activeSessionId: initial.id, killLocalShellSession: kill });
    useFileWorkspaceStore.setState({ tabs: [], activeTabId: null });
    usePluginWorkspaceStore.setState({ tabs: [], activeTabId: null, panels: {} });
    useDetachedOwnership.setState({ owners: {} });
    vi.stubGlobal('localStorage', { getItem: () => null, setItem: vi.fn(), removeItem: vi.fn(), clear: vi.fn() });
  });
  afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); fireEvent.mouseUp(document); });

  it('opens a new tab immediately after dragging another tab', () => {
    const create = vi.fn();
    render(<SessionTabs onNewSession={create} />);
    releaseDrag();
    fireEvent.click(screen.getByRole('button', { name: 'session.newSession' }), { detail: 1 });
    expect(create).toHaveBeenCalledTimes(1);
    expect(kill).not.toHaveBeenCalled();
  });
  it('allows the close button immediately after dragging and does not treat its press as a new drag', async () => {
    render(<SessionTabs />);
    releaseDrag();
    const close = screen.getByRole('button', { name: 'Close Test shell session' });
    fireEvent.mouseDown(close, { button: 0, buttons: 1, clientX: 450, clientY: 60 });
    fireEvent.mouseUp(close, { button: 0, clientX: 450, clientY: 60 });
    fireEvent.click(close, { detail: 1 });
    expect(kill).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'session.closeLocalShell' }));
    await waitFor(() => expect(kill).toHaveBeenCalledWith(initial.id));
    expect(useSessionStore.getState().sessions).toHaveLength(0);
  });
  it('cancels a close request, then allows opening and confirming it again', async () => {
    render(<SessionTabs />);
    fireEvent.click(screen.getByRole('button', { name: 'Close Test shell session' }));
    fireEvent.click(screen.getByRole('button', { name: 'common.cancel' }));
    expect(kill).not.toHaveBeenCalled();
    expect(useSessionStore.getState().sessions).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: 'Close Test shell session' }));
    fireEvent.click(screen.getByRole('button', { name: 'session.closeLocalShell' }));
    await waitFor(() => expect(kill).toHaveBeenCalledTimes(1));
  });
  it('closes a file tab without terminating its terminal', () => {
    useFileWorkspaceStore.getState().openFile({ sessionId: initial.id, path: '/test.ts', name: 'test.ts', size: 10 });
    render(<SessionTabs />);
    fireEvent.click(screen.getByRole('button', { name: 'Close test.ts' }), { detail: 1 });
    expect(useFileWorkspaceStore.getState().tabs).toHaveLength(0);
    expect(kill).not.toHaveBeenCalled();
  });
  it('does not leave a drag armed after a missed mouseup', () => {
    const create = vi.fn();
    render(<SessionTabs onNewSession={create} />);
    const tab = screen.getByRole('tab', { name: /Test shell/ });
    fireEvent.mouseDown(tab, { buttons: 1, clientX: 400, clientY: 60 });
    fireEvent.mouseMove(document, { buttons: 1, clientX: 450, clientY: 65 });
    fireEvent.mouseMove(document, { buttons: 0, clientX: 450, clientY: 65 });
    expect(document.body).not.toHaveClass('tab-dragging');
    fireEvent.click(screen.getByRole('button', { name: 'session.newSession' }), { detail: 1 });
    expect(create).toHaveBeenCalledTimes(1);
  });
});
