export interface TrackedInputState {
  buffer: string;
  cursor: number;
  known: boolean;
}

function clampCursor(cursor: number, buffer: string): number {
  return Math.max(0, Math.min(cursor, buffer.length));
}

export function applyTrackedInput(
  buffer: string,
  cursor: number,
  data: string
): TrackedInputState {
  const position = clampCursor(cursor, buffer);

  if (data === '\r' || data === '\n' || data === '\x03') {
    return { buffer: '', cursor: 0, known: true };
  }
  if (data === '\x1b[D') {
    return { buffer, cursor: Math.max(0, position - 1), known: true };
  }
  if (data === '\x1b[C') {
    return { buffer, cursor: Math.min(buffer.length, position + 1), known: true };
  }
  if (data === '\x1b[H' || data === '\x1bOH' || data === '\x01') {
    return { buffer, cursor: 0, known: true };
  }
  if (data === '\x1b[F' || data === '\x1bOF' || data === '\x05') {
    return { buffer, cursor: buffer.length, known: true };
  }
  if (data === '\x7f' || data === '\b') {
    if (position === 0) return { buffer, cursor: 0, known: true };
    return {
      buffer: buffer.slice(0, position - 1) + buffer.slice(position),
      cursor: position - 1,
      known: true,
    };
  }
  if (data === '\x1b[3~') {
    if (position >= buffer.length) return { buffer, cursor: position, known: true };
    return {
      buffer: buffer.slice(0, position) + buffer.slice(position + 1),
      cursor: position,
      known: true,
    };
  }
  if (data === '\x15') {
    return { buffer: buffer.slice(position), cursor: 0, known: true };
  }
  if (data === '\x0b') {
    return { buffer: buffer.slice(0, position), cursor: position, known: true };
  }
  if (data === '\x17') {
    const before = buffer.slice(0, position);
    const wordStart = before.search(/\S+\s*$/);
    const start = wordStart < 0 ? 0 : wordStart;
    return {
      buffer: buffer.slice(0, start) + buffer.slice(position),
      cursor: start,
      known: true,
    };
  }
  if (data && !/[\x00-\x1F\x7F]/.test(data)) {
    return {
      buffer: buffer.slice(0, position) + data + buffer.slice(position),
      cursor: position + data.length,
      known: true,
    };
  }

  return { buffer: '', cursor: 0, known: false };
}

export function getClickedInputPosition({
  clickColumn,
  clickRow,
  cursorColumn,
  cursorRow,
  terminalColumns,
  inputLength,
  inputCursor,
}: {
  clickColumn: number;
  clickRow: number;
  cursorColumn: number;
  cursorRow: number;
  terminalColumns: number;
  inputLength: number;
  inputCursor: number;
}): number {
  const cellDelta = ((clickRow - cursorRow) * terminalColumns) + clickColumn - cursorColumn;
  return Math.max(0, Math.min(inputLength, inputCursor + cellDelta));
}

export function getCursorMoveSequence(current: number, target: number): string {
  const distance = Math.abs(target - current);
  if (distance === 0) return '';
  return (target < current ? '\x1b[D' : '\x1b[C').repeat(distance);
}
