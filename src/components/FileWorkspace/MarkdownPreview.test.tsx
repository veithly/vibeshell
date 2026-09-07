import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MarkdownPreview, resolveDocumentPath } from './MarkdownPreview';
import type { FileWorkspaceTab } from '../../stores/fileWorkspaceStore';
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../lib/tauri', () => ({ safeInvoke: vi.fn(async () => ({ success: false, error: { message: 'not found' } })) }));
const tab: FileWorkspaceTab = { source: 'local', sessionId: 'local-files', id: 'test', path: '/docs/你好.md', name: '你好.md', kind: 'text', size: 20, dirty: false };
afterEach(cleanup);
describe('safe basic Markdown preview', () => {
  it('renders headings, lists, emphasis, code fences and tables', () => {
    render(<MarkdownPreview tab={tab} text={'# 标题\n\n**bold** and `code`\n\n- first\n- [x] done\n\n| A | B |\n| --- | --- |\n| one | two |\n\n```js\nconst x = 1;\n```'} />);
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('标题');
    expect(screen.getByText('bold').tagName).toBe('STRONG');
    expect(screen.getByRole('table')).toHaveTextContent('one');
    expect(screen.getByRole('checkbox')).toBeChecked();
    expect(screen.getByText('const x = 1;').closest('pre')).not.toBeNull();
  });
  it('renders HTML/MDX as text and strips unsafe link schemes', () => {
    const { container } = render(<MarkdownPreview tab={tab} text={'<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[bad](javascript:alert) [bad2](data:text/html,evil)'} />);
    expect(container.querySelector('script,img,iframe')).toBeNull();
    expect(container.querySelector('a[href^="javascript"],a[href^="data"]')).toBeNull();
    expect(container).toHaveTextContent('<script>alert(1)</script>');
  });
  it('requires explicit consent for external images', () => {
    render(<MarkdownPreview tab={tab} text={'![photo](https://example.com/photo.png)'} />);
    expect(screen.queryByRole('img')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /markdown.loadImage/ }));
    expect(screen.getByRole('img')).toHaveAttribute('referrerpolicy', 'no-referrer');
  });
  it('resolves escaped local filenames without accepting URL handlers', () => {
    expect(resolveDocumentPath('images/picture%20one.png', tab.path)).toBe('/docs/images/picture one.png');
    expect(resolveDocumentPath('javascript:alert(1)', tab.path)).toBeNull();
    expect(resolveDocumentPath('//example.com/image.png', tab.path)).toBeNull();
  });
});
