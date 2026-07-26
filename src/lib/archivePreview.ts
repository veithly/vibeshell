import { Gunzip } from 'fflate';

export interface ArchiveEntry {
  path: string;
  size: number;
  compressedSize: number | null;
  isDirectory: boolean;
}

const MAX_ARCHIVE_ENTRIES = 5_000;
const MAX_EXPANDED_TAR_BYTES = 128 * 1024 * 1024;
const textDecoder = new TextDecoder();

function readNullTerminated(bytes: Uint8Array): string {
  const end = bytes.indexOf(0);
  return textDecoder.decode(end === -1 ? bytes : bytes.subarray(0, end)).trim();
}

function listZipEntries(bytes: Uint8Array): ArchiveEntry[] {
  if (bytes.length < 22) throw new Error('Invalid ZIP archive');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const minimumOffset = Math.max(0, bytes.length - 65_557);
  let eocdOffset = -1;

  for (let offset = bytes.length - 22; offset >= minimumOffset; offset -= 1) {
    if (view.getUint32(offset, true) === 0x06054b50) {
      eocdOffset = offset;
      break;
    }
  }
  if (eocdOffset === -1) throw new Error('ZIP central directory was not found');

  const totalEntries = view.getUint16(eocdOffset + 10, true);
  const centralSize = view.getUint32(eocdOffset + 12, true);
  const centralOffset = view.getUint32(eocdOffset + 16, true);
  if (totalEntries === 0xffff || centralOffset + centralSize > bytes.length) {
    throw new Error('ZIP64 or damaged ZIP archives are not supported');
  }

  const entries: ArchiveEntry[] = [];
  let offset = centralOffset;
  for (let index = 0; index < totalEntries && entries.length < MAX_ARCHIVE_ENTRIES; index += 1) {
    if (offset + 46 > bytes.length || view.getUint32(offset, true) !== 0x02014b50) {
      throw new Error('Invalid ZIP central-directory entry');
    }

    const flags = view.getUint16(offset + 8, true);
    const compressedSize = view.getUint32(offset + 20, true);
    const size = view.getUint32(offset + 24, true);
    const nameLength = view.getUint16(offset + 28, true);
    const extraLength = view.getUint16(offset + 30, true);
    const commentLength = view.getUint16(offset + 32, true);
    const nextOffset = offset + 46 + nameLength + extraLength + commentLength;
    if (nextOffset > bytes.length) throw new Error('Truncated ZIP central directory');

    const nameBytes = bytes.subarray(offset + 46, offset + 46 + nameLength);
    const path = (flags & 0x0800) !== 0
      ? textDecoder.decode(nameBytes)
      : textDecoder.decode(nameBytes);
    entries.push({
      path,
      size,
      compressedSize,
      isDirectory: path.endsWith('/'),
    });
    offset = nextOffset;
  }

  return entries;
}

function parseTarSize(bytes: Uint8Array): number {
  if ((bytes[0] & 0x80) !== 0) {
    throw new Error('Binary TAR size fields are not supported');
  }
  const value = readNullTerminated(bytes).replace(/\s/g, '');
  if (!value) return 0;
  const size = Number.parseInt(value, 8);
  if (!Number.isSafeInteger(size) || size < 0) throw new Error('Invalid TAR entry size');
  return size;
}

function listTarEntries(bytes: Uint8Array): ArchiveEntry[] {
  const entries: ArchiveEntry[] = [];
  let offset = 0;

  while (offset + 512 <= bytes.length && entries.length < MAX_ARCHIVE_ENTRIES) {
    const header = bytes.subarray(offset, offset + 512);
    if (header.every((value) => value === 0)) break;

    const name = readNullTerminated(header.subarray(0, 100));
    const prefix = readNullTerminated(header.subarray(345, 500));
    const path = prefix ? `${prefix}/${name}` : name;
    const size = parseTarSize(header.subarray(124, 136));
    const type = String.fromCharCode(header[156] || 48);
    if (path && !['g', 'x', 'L', 'K'].includes(type)) {
      entries.push({
        path,
        size,
        compressedSize: null,
        isDirectory: type === '5' || path.endsWith('/'),
      });
    }

    const paddedSize = Math.ceil(size / 512) * 512;
    const nextOffset = offset + 512 + paddedSize;
    if (nextOffset <= offset || nextOffset > bytes.length) {
      throw new Error('Truncated TAR archive');
    }
    offset = nextOffset;
  }

  return entries;
}

function gunzipWithLimit(bytes: Uint8Array): Uint8Array {
  const chunks: Uint8Array[] = [];
  let expandedSize = 0;
  const gunzip = new Gunzip((chunk) => {
    expandedSize += chunk.length;
    if (expandedSize > MAX_EXPANDED_TAR_BYTES) {
      throw new Error('Expanded archive is too large to preview');
    }
    chunks.push(chunk);
  });
  gunzip.push(bytes, true);

  const result = new Uint8Array(expandedSize);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}

export async function listArchiveEntries(bytes: Uint8Array, filename: string): Promise<ArchiveEntry[]> {
  const lowerName = filename.toLowerCase();
  if (lowerName.endsWith('.zip')) return listZipEntries(bytes);
  if (lowerName.endsWith('.tar')) return listTarEntries(bytes);
  if (lowerName.endsWith('.tar.gz') || lowerName.endsWith('.tgz')) {
    return listTarEntries(gunzipWithLimit(bytes));
  }
  throw new Error('This archive format can be downloaded, but its contents cannot be listed yet');
}

export function decodeBase64(base64: string): Uint8Array {
  const binary = window.atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}
