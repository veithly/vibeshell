import { describe, expect, it } from 'vitest';
import { gzipSync } from 'fflate';
import { listArchiveEntries } from './archivePreview';

const encoder = new TextEncoder();

function writeText(target: Uint8Array, offset: number, value: string) {
  target.set(encoder.encode(value), offset);
}

function makeZipDirectory(): Uint8Array {
  const name = encoder.encode('docs/readme.txt');
  const centralSize = 46 + name.length;
  const bytes = new Uint8Array(centralSize + 22);
  const view = new DataView(bytes.buffer);

  view.setUint32(0, 0x02014b50, true);
  view.setUint16(8, 0x0800, true);
  view.setUint32(20, 12, true);
  view.setUint32(24, 24, true);
  view.setUint16(28, name.length, true);
  bytes.set(name, 46);

  view.setUint32(centralSize, 0x06054b50, true);
  view.setUint16(centralSize + 8, 1, true);
  view.setUint16(centralSize + 10, 1, true);
  view.setUint32(centralSize + 12, centralSize, true);
  view.setUint32(centralSize + 16, 0, true);
  return bytes;
}

function makeTar(): Uint8Array {
  const bytes = new Uint8Array(1536);
  writeText(bytes, 0, 'src/main.ts');
  writeText(bytes, 124, '00000000004\0');
  bytes[156] = '0'.charCodeAt(0);
  writeText(bytes, 257, 'ustar\0');
  writeText(bytes, 512, 'test');
  return bytes;
}

describe('listArchiveEntries', () => {
  it('reads ZIP central-directory metadata without expanding files', async () => {
    await expect(listArchiveEntries(makeZipDirectory(), 'bundle.zip')).resolves.toEqual([
      { path: 'docs/readme.txt', size: 24, compressedSize: 12, isDirectory: false },
    ]);
  });

  it('reads tar and tar.gz entry headers', async () => {
    const tar = makeTar();
    const expected = [{ path: 'src/main.ts', size: 4, compressedSize: null, isDirectory: false }];

    await expect(listArchiveEntries(tar, 'bundle.tar')).resolves.toEqual(expected);
    await expect(listArchiveEntries(gzipSync(tar), 'bundle.tar.gz')).resolves.toEqual(expected);
  });
});
