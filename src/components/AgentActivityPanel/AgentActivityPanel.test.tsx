import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
import { useAgentActivityStore } from '../../stores/agentActivityStore';
import { useSessionStore } from '../../stores/sessionStore';
import { AgentActivityPanel } from './AgentActivityPanel';
const refresh = useAgentActivityStore.getState().refresh;
const loadOlder = useAgentActivityStore.getState().loadOlder;
afterEach(() => { cleanup(); useAgentActivityStore.setState({ refresh, loadOlder }); });
beforeEach(() => {
  useAgentActivityStore.setState({ activities: [], error: null, hasOlder: false, refresh: vi.fn(async () => false) });
  useSessionStore.setState({ sessions: [{ id: 'human', serverName: 'Human', serverId: 'one', state: 'connected', createdAt: 1, sessionType: 'ssh' },
    { id: 'agent', serverName: 'Agent session', serverId: 'two', state: 'connected', createdAt: 2, sessionType: 'ssh' }], activeSessionId: 'human' });
});
it('labels only explicit CLI rows as Agent and does not render an always-on notice', () => {
  useAgentActivityStore.setState({activities: (['cli', 'user', 'legacy'] as const).map((origin, i) => ({origin, sequence: i+1, id: origin, tool: 'cli.exec', summary: `cmd-${origin}`, status: 'succeeded', timestamp: i}))});
  const props = {onClose: () => {}, onSessionsChanged: () => {}};
  const {rerender} = render(<AgentActivityPanel {...props} open={false} />);
  expect(screen.queryByText('cmd-cli')).not.toBeInTheDocument();
  rerender(<AgentActivityPanel {...props} open />);
  expect(screen.getByText('cmd-cli')).toHaveClass('text-tokyo-magenta');
  expect(screen.getByText('cmd-user')).toHaveClass('text-tokyo-cyan');
  expect(screen.getAllByText('agentActivity.cli')).toHaveLength(1);
  expect(screen.getByText('agentActivity.legacy')).toBeInTheDocument();
});
it('renders untruncated multiline history, earlier-page control and an explicit session navigation link', () => {
  const command = `printf '${'x'.repeat(500)}'\nwhoami`;
  const older = vi.fn(async () => {});
  useAgentActivityStore.setState({ hasOlder: true, loadOlder: older, activities: [{ sequence: 2, id: 'run', tool: 'cli.exec', summary: command, status: 'succeeded', sessionId: 'agent', timestamp: 1 }] });
  const { container } = render(<AgentActivityPanel open onClose={() => {}} onSessionsChanged={() => {}} />);
  expect(container.querySelector('pre')?.textContent).toBe(command);
  expect(useSessionStore.getState().activeSessionId).toBe('human');
  fireEvent.click(screen.getByRole('button', { name: /Agent session/ }));
  expect(useSessionStore.getState().activeSessionId).toBe('agent');
  fireEvent.click(screen.getByRole('button', { name: 'agentActivity.earlier' }));
  expect(older).toHaveBeenCalledOnce();
});
