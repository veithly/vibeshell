import { describe, expect, it } from 'vitest';
import type { PluginAction, PluginRecord } from './types';
import { isPluginCompatible, parsePluginTable } from './pluginUtils';

const action: PluginAction = {
  id: 'containers',
  name: 'Containers',
  description: 'List containers',
  program: 'docker',
  args: ['ps'],
  inputs: [],
  requiresConfirmation: false,
  elevate: false,
  allowSudo: false,
  output: {
    kind: 'table',
    columns: ['ID', 'Name', 'Status'],
    delimiter: '\t',
  },
};

describe('plugin table parsing', () => {
  it('maps delimited output to the declared columns', () => {
    expect(parsePluginTable(action, 'a1\tapi\tUp 2 hours\nb2\tworker\tExited\n')).toEqual({
      rows: [
        ['a1', 'api', 'Up 2 hours'],
        ['b2', 'worker', 'Exited'],
      ],
      truncated: false,
    });
  });

  it('keeps overflow text in the final column', () => {
    expect(parsePluginTable(action, 'a1\tapi\tUp\thealthy')).toEqual({
      rows: [['a1', 'api', 'Up\thealthy']],
      truncated: false,
    });
  });

  it('caps table rows before React renders them', () => {
    const output = Array.from({ length: 1002 }, (_, index) => `${index}\tapi\tUp`).join('\n');
    const parsed = parsePluginTable(action, output);
    expect(parsed.rows).toHaveLength(1000);
    expect(parsed.truncated).toBe(true);
  });

  it('does not expand delimiter-dense lines beyond declared columns', () => {
    const parsed = parsePluginTable(action, `a\tb\t${'x\t'.repeat(10_000)}`);
    expect(parsed.rows).toHaveLength(1);
    expect(parsed.rows[0]).toHaveLength(3);
    expect(parsed.rows[0][2].startsWith('x\tx\t')).toBe(true);
  });
});

describe('plugin session compatibility', () => {
  const plugin = {
    installed: true,
    enabled: true,
    manifest: { sessionTypes: ['ssh'] },
  } as PluginRecord;

  it('requires installation, enablement, and a matching session type', () => {
    expect(isPluginCompatible(plugin, 'ssh')).toBe(true);
    expect(isPluginCompatible(plugin, 'local')).toBe(false);
    expect(isPluginCompatible({ ...plugin, enabled: false }, 'ssh')).toBe(false);
  });
});
