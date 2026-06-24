import { describe, expect, it } from 'vitest';
import { compareVersions, isNewerVersion, selectPreferredAsset } from './updateStore';

describe('updateStore version helpers', () => {
  it('compares semantic versions numerically', () => {
    expect(compareVersions('0.1.10', '0.1.9')).toBe(1);
    expect(compareVersions('v1.2.0', '1.2')).toBe(0);
    expect(compareVersions('1.2.3', '1.3.0')).toBe(-1);
  });

  it('only reports candidates newer than the current version', () => {
    expect(isNewerVersion('0.1.19', '0.1.18')).toBe(true);
    expect(isNewerVersion('0.1.18', '0.1.18')).toBe(false);
    expect(isNewerVersion('0.1.17', '0.1.18')).toBe(false);
    expect(isNewerVersion('0.1.19', null)).toBe(false);
  });
});

describe('updateStore release asset selection', () => {
  const assets = [
    { name: 'VibeShell_0.1.19_x64.dmg', downloadUrl: 'https://example.com/mac' },
    { name: 'VibeShell_0.1.19_x64.msi', downloadUrl: 'https://example.com/win' },
    { name: 'latest.json', downloadUrl: 'https://example.com/latest.json' },
    { name: 'VibeShell_0.1.19_amd64.AppImage', downloadUrl: 'https://example.com/linux' },
  ];

  it('prefers Windows installers on Windows', () => {
    expect(selectPreferredAsset(assets, 'Windows NT')?.downloadUrl).toBe('https://example.com/win');
  });

  it('prefers macOS installers on macOS', () => {
    expect(selectPreferredAsset(assets, 'Macintosh; Intel Mac OS X')?.downloadUrl).toBe(
      'https://example.com/mac'
    );
  });

  it('skips metadata assets when selecting a download', () => {
    expect(selectPreferredAsset([assets[2]], 'Windows NT')).toBeNull();
  });
});
