import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
import { useAgentActivityStore } from '../../stores/agentActivityStore';
import { useSessionStore } from '../../stores/sessionStore';
import { AgentActivityNotice, AgentActivityPanel } from './AgentActivityPanel';
const refresh = useAgentActivityStore.getState().refresh;
const loadOlder = useAgentActivityStore.getState().loadOlder;
afterEach(() => { cleanup(); useAgentActivityStore.setState({ refresh, loadOlder }); });
beforeEach(() => {
  useAgentActivityStore.setState({ activities: [], error: null, hasOlder: false, refresh: vi.fn(async () => false) });
  useSessionStore.setState({ sessions: [{ id: 'human', serverName: 'Human', serverId: 'one', state: 'connected', createdAt: 1, sessionType: 'ssh' },
    { id: 'agent', serverName: 'Agent session', serverId: 'two', state: 'connected', createdAt: 2, sessionType: 'ssh' }], activeSessionId: 'human' });
});
it('shows the latest complete command in a clickable notice without requiring the panel to be open', () => {
  useAgentActivityStore.setState({ activities: [{ sequence: 1, id: 'run', tool: 'cli.exec', summary: 'printf visible-marker', status: 'started', sessionId: 'agent', timestamp: 1 }] });
  const open = vi.fn();
  render(<AgentActivityNotice onOpen={open} />);
  expect(screen.getByText('printf visible-marker')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Open Agent command history' }));
  expect(open).toHaveBeenCalledOnce();
  expect(useSessionStore.getState().activeSessionId).toBe('human');
});
it('renders untruncated multiline history, earlier-page control and an explicit session navigation link', () => {
  const command = `printf '${'x'.repeat(500)}'\nwhoami`;
  const older = vi.fn(async () => {});
  useAgentActivityStore.setState({ hasOlder: true, loadOlder: older, activities: [{ sequence: 2, id: 'run', tool: 'cli.exec', summary: command, status: 'succeeded', sessionId: 'agent', timestamp: 1 }] });
  const { container } = render(<AgentActivityPanel open onClose={() => {}} onSessionsChanged={() => {}} />);
  expect(container.querySelector('pre')?.textContent).toBe(command);
  expect(useSessionStore.getState().activeSessionId).toBe('human');
  fireEvent.click(screen.getByRole('button', { name: 'Agent session' }));
  expect(useSessionStore.getState().activeSessionId).toBe('agent');
  fireEvent.click(screen.getByRole('button', { name: 'Load earlier commands' }));
  expect(older).toHaveBeenCalledOnce();
});
