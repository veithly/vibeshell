import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppearanceModeControl } from './AppearanceModeControl';
import { defaultSettings, useSettingsStore } from '../../stores/settingsStore';

vi.mock('react-i18next', () => ({useTranslation: () => ({t: (key: string) => key})}));
vi.mock('../../lib/tauri', () => ({invokeOrThrow: vi.fn(async () => null)}));

describe('appearance mode settings', () => {
  beforeEach(() => useSettingsStore.setState({settings: structuredClone(defaultSettings), initialized: true}));
  afterEach(cleanup);
  it('selects night, day and system from the settings control', async () => {
    render(<AppearanceModeControl />);
    fireEvent.click(screen.getByRole('button', {name: 'settings.nightMode'}));
    await waitFor(() => expect(screen.getByRole('button', {name: 'settings.nightMode'})).toHaveAttribute('aria-pressed', 'true'));
    expect(useSettingsStore.getState().settings.appearance.theme).toBe('ink-black');
    fireEvent.click(screen.getByRole('button', {name: 'settings.dayMode'}));
    expect(useSettingsStore.getState().settings.appearance.theme).toBe('paper-white');
    fireEvent.click(screen.getByRole('button', {name: 'settings.themeModeSystem'}));
    expect(useSettingsStore.getState().settings.appearance.themeMode).toBe('system');
  });
});
