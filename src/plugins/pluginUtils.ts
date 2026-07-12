import type {
  PluginAction,
  PluginRecord,
  PluginSessionType,
} from './types';

export const MAX_PLUGIN_TABLE_ROWS = 1_000;

export interface ParsedPluginTable {
  rows: string[][];
  truncated: boolean;
}

export function isPluginCompatible(
  plugin: PluginRecord,
  sessionType: PluginSessionType
): boolean {
  return plugin.installed
    && plugin.enabled
    && plugin.manifest.sessionTypes.includes(sessionType);
}

export function parsePluginTable(action: PluginAction, output: string): ParsedPluginTable {
  if (action.output.kind !== 'table') return { rows: [], truncated: false };

  const delimiter = action.output.delimiter || '\t';
  const width = action.output.columns.length;
  const rows: string[][] = [];
  let cursor = 0;
  let truncated = false;

  while (cursor <= output.length) {
    const newline = output.indexOf('\n', cursor);
    const end = newline === -1 ? output.length : newline;
    const line = output.slice(cursor, end).replace(/\r$/, '');
    cursor = newline === -1 ? output.length + 1 : newline + 1;
    if (line.trim().length === 0) continue;

    if (rows.length === MAX_PLUGIN_TABLE_ROWS) {
      truncated = true;
      break;
    }

    const cells: string[] = [];
    let cellStart = 0;
    while (cells.length < width - 1) {
      const separator = line.indexOf(delimiter, cellStart);
      if (separator === -1) break;
      cells.push(line.slice(cellStart, separator));
      cellStart = separator + delimiter.length;
    }
    cells.push(line.slice(cellStart));
    while (cells.length < width) cells.push('');
    rows.push(cells);
  }

  return { rows, truncated };
}
