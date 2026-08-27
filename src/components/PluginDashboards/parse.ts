/** Parsers that turn plugin action output into dashboard-ready structures. */

/** Split delimiter-separated output (e.g. docker --format with tabs) into rows. */
export function parseDelimited(output: string, delimiter = '\t'): string[][] {
  return output
    .split('\n')
    .map((line) => line.replace(/\r$/, ''))
    .filter((line) => line.trim().length > 0)
    .map((line) => line.split(delimiter));
}

/** Parse a header line plus CSV rows (psql --csv, sqlite3 -csv). */
export function parseCsvTable(output: string): { columns: string[]; rows: string[][] } {
  const lines = output.split('\n').map((line) => line.replace(/\r$/, ''));
  while (lines.length > 0 && lines[lines.length - 1].trim() === '') lines.pop();
  if (lines.length === 0) return { columns: [], rows: [] };

  const records: string[][] = [];
  let field = '';
  let record: string[] = [];
  let inQuotes = false;
  for (let i = 0; i < output.length; i++) {
    const ch = output[i];
    if (inQuotes) {
      if (ch === '"') {
        if (output[i + 1] === '"') {
          field += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        field += ch;
      }
      continue;
    }
    if (ch === '"') {
      inQuotes = true;
    } else if (ch === ',') {
      record.push(field);
      field = '';
    } else if (ch === '\n') {
      record.push(field.replace(/\r$/, ''));
      records.push(record);
      record = [];
      field = '';
    } else {
      field += ch;
    }
  }
  if (field.length > 0 || record.length > 0) {
    record.push(field.replace(/\r$/, ''));
    records.push(record);
  }

  const [header, ...rows] = records.filter((entry) => entry.length > 1 || entry[0] !== '');
  return { columns: header ?? [], rows };
}

/** Parse redis-cli INFO output into a flat key→value map. */
export function parseRedisInfo(output: string): Record<string, string> {
  const map: Record<string, string> = {};
  for (const line of output.split('\n')) {
    if (!line || line.startsWith('#')) continue;
    const separator = line.indexOf(':');
    if (separator === -1) continue;
    map[line.slice(0, separator)] = line.slice(separator + 1).trim();
  }
  return map;
}

/** Total key count from `INFO keyspace` (`db0:keys=12,expires=3,avg_ttl=0`). */
export function parseRedisKeyspace(output: string): number {
  let total = 0;
  for (const line of output.split('\n')) {
    const match = line.match(/keys=(\d+)/);
    if (match) total += Number(match[1]);
  }
  return total;
}

/** Line list output (one value per line, e.g. database names or scanned keys). */
export function parseLines(output: string): string[] {
  return output
    .split('\n')
    .map((line) => line.replace(/\r$/, ''))
    .filter((line) => line.trim().length > 0);
}
