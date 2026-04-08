import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { PreviewModal } from '../PreviewModal';

const safeInvokeMock = vi.fn();

vi.mock('../../../lib/tauri', () => ({
  safeInvoke: (...args: unknown[]) => safeInvokeMock(...args),
}));

describe('PreviewModal keyboard shortcuts', () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    safeInvokeMock.mockReset();
    safeInvokeMock.mockResolvedValue({
      success: true,
      data: {
        content: 'line-1\nline-2',
        isBinary: false,
        size: 12,
        truncated: false,
        mimeType: 'text/plain',
      },
    });
  });

  it.each(['c', 'v', 'x', 'a'])(
    'preserves native Ctrl/Cmd+%s behavior in textarea',
    async (key) => {
      render(
        <PreviewModal
          isOpen
          filePath="/tmp/demo.txt"
          fileName="demo.txt"
          fileSize={12}
          sessionId="s1"
          onClose={() => {}}
          onSave={vi.fn().mockResolvedValue(undefined)}
        />
      );

      await waitFor(() => {
        expect(screen.getByTitle('Edit file')).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTitle('Edit file'));
      const editor = await screen.findByRole('textbox');
      editor.focus();

      const event = new KeyboardEvent('keydown', {
        key,
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      });

      editor.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(false);
    }
  );

  it('handles Ctrl/Cmd+S to save while editing', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);

    render(
      <PreviewModal
        isOpen
        filePath="/tmp/demo.txt"
        fileName="demo.txt"
        fileSize={12}
        sessionId="s1"
        onClose={() => {}}
        onSave={onSave}
      />
    );

    await waitFor(() => {
      expect(screen.getByTitle('Edit file')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTitle('Edit file'));
    const editor = await screen.findByRole('textbox');

    fireEvent.change(editor, { target: { value: 'line-1\nline-2\nline-3' } });

    const saveEvent = new KeyboardEvent('keydown', {
      key: 's',
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    });

    editor.dispatchEvent(saveEvent);

    expect(saveEvent.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith('line-1\nline-2\nline-3');
    });
  });
});
