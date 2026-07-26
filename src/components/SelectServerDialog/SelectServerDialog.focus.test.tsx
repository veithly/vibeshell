import { useState } from 'react';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SelectServerDialog } from './SelectServerDialog';

const mocks = vi.hoisted(() => ({
  fetchServers: vi.fn(async () => undefined),
  fetchGroups: vi.fn(async () => undefined),
  fetchAvailableShells: vi.fn(async () => undefined),
  fetchDefaultShell: vi.fn(async () => undefined),
  setLastSelectedShell: vi.fn(),
  createLocalShellSession: vi.fn(async () => null),
  setActiveSession: vi.fn(),
  loadRuntimeCapabilities: vi.fn(async () => ({ localShell: true })),
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock('@gsap/react', () => ({
  useGSAP: vi.fn(),
}));

vi.mock('gsap', () => ({
  default: {
    registerPlugin: vi.fn(),
    fromTo: vi.fn(),
  },
}));

vi.mock('../../stores/serverStore', () => ({
  useServerStore: () => ({
    servers: [],
    groups: [],
    fetchServers: mocks.fetchServers,
    fetchGroups: mocks.fetchGroups,
  }),
}));

vi.mock('../../stores/sessionStore', () => ({
  useSessionStore: (selector: (state: unknown) => unknown) => selector({
    sessions: [],
    createLocalShellSession: mocks.createLocalShellSession,
    setActiveSession: mocks.setActiveSession,
  }),
}));

vi.mock('../../stores/localShellStore', () => ({
  useAvailableShells: () => ({ shells: [] }),
  useLocalShellStore: (selector: (state: unknown) => unknown) => selector({
    fetchAvailableShells: mocks.fetchAvailableShells,
    fetchDefaultShell: mocks.fetchDefaultShell,
    setLastSelectedShell: mocks.setLastSelectedShell,
  }),
}));

vi.mock('../../stores/runtimeCapabilitiesStore', () => ({
  useRuntimeCapabilitiesStore: (selector: (state: unknown) => unknown) => selector({
    capabilities: { localShell: true },
    status: 'ready',
    load: mocks.loadRuntimeCapabilities,
  }),
}));

vi.mock('../CodingAgentLauncher', () => ({
  CodingAgentLauncher: () => <button type="button">Launch agent</button>,
}));

function DialogHarness({ renderVersion = 0 }: { renderVersion?: number }) {
  const [open, setOpen] = useState(false);

  return (
    <>
      <span data-testid="render-version">{renderVersion}</span>
      <button type="button" onClick={() => setOpen(true)}>Open agent launcher</button>
      <SelectServerDialog
        isOpen={open}
        initialTab="agent"
        onClose={() => setOpen(false)}
        onSelectServer={vi.fn()}
        onAddServer={vi.fn()}
      />
    </>
  );
}

afterEach(cleanup);

describe('SelectServerDialog agent focus lifecycle', () => {
  it('traps focus and restores it to the trigger on close', async () => {
    const { rerender } = render(<DialogHarness />);

    const trigger = screen.getByRole('button', { name: 'Open agent launcher' });
    trigger.focus();
    fireEvent.click(trigger);

    const dialog = await screen.findByRole('dialog');
    await waitFor(() => expect(dialog).toContainElement(document.activeElement as HTMLElement));

    const focusedBeforeParentRender = document.activeElement;
    rerender(<DialogHarness renderVersion={1} />);
    expect(document.activeElement).toBe(focusedBeforeParentRender);
    expect(dialog).toContainElement(document.activeElement as HTMLElement);

    const focusable = within(dialog).getAllByRole('button');
    const first = focusable[0];
    const last = focusable[focusable.length - 1];

    last.focus();
    fireEvent.keyDown(window, { key: 'Tab' });
    expect(first).toHaveFocus();
    expect(dialog).toContainElement(document.activeElement as HTMLElement);

    first.focus();
    fireEvent.keyDown(window, { key: 'Tab', shiftKey: true });
    expect(last).toHaveFocus();
    expect(dialog).toContainElement(document.activeElement as HTMLElement);

    fireEvent.click(within(dialog).getByRole('button', { name: 'common.close' }));

    await waitFor(() => expect(trigger).toHaveFocus());
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
