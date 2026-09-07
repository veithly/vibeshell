import { createElement, memo, useDeferredValue, useEffect, useMemo, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { safeInvoke } from '../../lib/tauri';
import { getBrowserMimeType, getFileViewerKind } from '../../lib/fileWorkspace';
import { decodeBase64 } from '../../lib/archivePreview';
import { openLocalFiles } from '../../lib/localFiles';
import { useFileWorkspaceStore, type FileWorkspaceTab } from '../../stores/fileWorkspaceStore';
import './markdown.css';

const PREVIEW_CHAR_LIMIT = 512 * 1024;
export function resolveDocumentPath(link: string, documentPath: string): string | null {
  if (!link || /[\u0000-\u001f\\]/.test(link) || /^[a-z][a-z\d+.-]*:/i.test(link) || link.startsWith('//')) return null;
  try {
    const base = `file://${documentPath.split('/').map(encodeURIComponent).join('/')}`;
    const target = new URL(link, base);
    return target.protocol === 'file:' && !target.host ? decodeURIComponent(target.pathname) : null;
  } catch { return null; }
}
function externalUrl(value: string): string | null {
  try { const url = new URL(value); return ['https:', 'http:'].includes(url.protocol) ? url.href : null; }
  catch { return null; }
}
function DocumentImage({ src, alt, tab }: { src: string; alt: string; tab: FileWorkspaceTab }) {
  const { t } = useTranslation();
  const external = externalUrl(src);
  const [approvedSource, setApprovedSource] = useState<string | null>(null);
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    let disposed = false; let objectUrl: string | undefined;
    setUrl(null); setError('');
    if (external) return;
    const path = resolveDocumentPath(src, tab.path);
    const directory = tab.path.slice(0, tab.path.lastIndexOf('/') + 1);
    if (!path || !path.startsWith(directory) || getFileViewerKind(path) !== 'image') { setError('blocked-image'); return; }
    void safeInvoke<{ content: string; mimeType: string }>(tab.source === 'local' ? 'local_file_read' : 'sftp_read_file', {
      request: { path, sessionId: tab.sessionId, asBinary: true, maxSize: 8 * 1024 * 1024 },
    }).then(result => {
      if (disposed) return;
      if (!result.success) { setError(result.error.message); return; }
      objectUrl = URL.createObjectURL(new Blob([decodeBase64(result.data.content)], { type: getBrowserMimeType(path, result.data.mimeType) }));
      setUrl(objectUrl);
    }).catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; if (objectUrl) URL.revokeObjectURL(objectUrl); };
  }, [src, external, tab.source, tab.path, tab.sessionId]);
  if (external && approvedSource !== external) return <button type="button" className="workspace-action" onClick={() => setApprovedSource(external)} title={external}>{t('markdown.loadImage')}: {alt || t('markdown.image')}</button>;
  if (error) return <span role="note" className="text-tokyo-comment">[{alt || t('markdown.image')}: {error === 'blocked-image' ? t('markdown.blockedImage') : error}]</span>;
  return (external || url) ? <img src={external ?? url!} alt={alt} loading="lazy" referrerPolicy="no-referrer" /> : <span>{alt}</span>;
}
function DocumentLink({ href, children, tab }: { href: string; children: ReactNode; tab: FileWorkspaceTab }) {
  const external = externalUrl(href);
  const path = resolveDocumentPath(href, tab.path);
  if (!external && !path && !href.startsWith('#')) return <span>{children}</span>;
  return <a href={external ?? href} onClick={event => {
    event.preventDefault();
    if (href.startsWith('#')) {
      try { document.getElementById(decodeURIComponent(href.slice(1)))?.scrollIntoView({ block: 'start' }); } catch { /* malformed anchor */ }
    } else if (external) void safeInvoke('open_external_url', { url: external });
    else if (path && tab.source === 'local') void openLocalFiles([path]);
    else if (path) useFileWorkspaceStore.getState().openFile({ sessionId: tab.sessionId, path, name: path.split('/').pop()!, size: 0 });
  }}>{children}</a>;
}

/** Deliberately bounded, basic Markdown, not an MDX or HTML execution engine. */
function inline(text: string, tab: FileWorkspaceTab, depth = 0): ReactNode {
  if (depth > 4 || text.length > 4096) return text;
  const pattern = /(`[^`\n]+`|!\[[^\]\n]*\]\([^\s)]+\)|\[[^\]\n]+\]\([^\s)]+\)|\*\*[^*\n]+\*\*|__[^_\n]+__|~~[^~\n]+~~|\*[^*\n]+\*|_[^_\n]+_)/g;
  const output: ReactNode[] = []; let start = 0; let match: RegExpExecArray | null;
  while ((match = pattern.exec(text))) {
    output.push(text.slice(start, match.index)); const token = match[0]; const key = match.index;
    const link = /^(!?)\[([^\]]*)\]\(([^)]+)\)$/.exec(token);
    if (link) output.push(link[1] ? <DocumentImage key={key} src={link[3]} alt={link[2]} tab={tab} /> : <DocumentLink key={key} href={link[3]} tab={tab}>{inline(link[2], tab, depth + 1)}</DocumentLink>);
    else if (token[0] === '`') output.push(<code key={key}>{token.slice(1, -1)}</code>);
    else if (token.startsWith('~~')) output.push(<del key={key}>{inline(token.slice(2, -2), tab, depth + 1)}</del>);
    else if (token.startsWith('**') || token.startsWith('__')) output.push(<strong key={key}>{inline(token.slice(2, -2), tab, depth + 1)}</strong>);
    else output.push(<em key={key}>{inline(token.slice(1, -1), tab, depth + 1)}</em>);
    start = pattern.lastIndex;
  }
  output.push(text.slice(start)); return output;
}
function cells(line: string): string[] { return line.trim().replace(/^\||\|$/g, '').split('|').map(cell => cell.trim()); }
function blocks(source: string, tab: FileWorkspaceTab, depth = 0): ReactNode[] {
  if (depth > 8) return [source];
  const lines = source.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n').split('\n');
  const output: ReactNode[] = [];
  for (let index = 0; index < lines.length;) {
    const line = lines[index]; const key = index;
    if (!line.trim()) { index++; continue; }
    const fence = /^\s{0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (fence) {
      const code: string[] = []; index++;
      const close = new RegExp(`^\\s{0,3}${fence[1][0]}{${fence[1].length},}\\s*$`);
      while (index < lines.length && !close.test(lines[index])) code.push(lines[index++]);
      if (index < lines.length) index++;
      output.push(<pre key={key}><code>{code.join('\n')}</code></pre>); continue;
    }
    const heading = /^(#{1,6})\s+(.+?)\s*#*\s*$/.exec(line);
    const setext = index + 1 < lines.length && /^\s*(={3,}|-{3,})\s*$/.test(lines[index + 1]);
    if (heading || setext) {
      const text = heading ? heading[2] : line;
      const level = heading ? heading[1].length : lines[index + 1].trim().startsWith('=') ? 1 : 2;
      output.push(createElement(`h${level}`, { key, id: text.toLowerCase().replace(/[^\p{L}\p{N}\s-]/gu, '').trim().replace(/\s+/g, '-') }, inline(text, tab)));
      index += heading ? 1 : 2; continue;
    }
    if (/^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/.test(line)) { output.push(<hr key={key} />); index++; continue; }
    if (line.includes('|') && index + 1 < lines.length && cells(lines[index + 1]).every(cell => /^:?-{3,}:?$/.test(cell))) {
      const headings = cells(line); index += 2; const rows: string[][] = [];
      while (index < lines.length && lines[index].includes('|') && lines[index].trim()) rows.push(cells(lines[index++]));
      output.push(<div key={key} className="markdown-table"><table><thead><tr>{headings.map((value, col) => <th key={col}>{inline(value, tab)}</th>)}</tr></thead><tbody>{rows.map((row, i) => <tr key={i}>{headings.map((_, col) => <td key={col}>{inline(row[col] ?? '', tab)}</td>)}</tr>)}</tbody></table></div>); continue;
    }
    if (/^\s{0,3}>/.test(line)) {
      const quote: string[] = [];
      while (index < lines.length && /^\s{0,3}>/.test(lines[index])) quote.push(lines[index++].replace(/^\s{0,3}>\s?/, ''));
      output.push(<blockquote key={key}>{blocks(quote.join('\n'), tab, depth + 1)}</blockquote>); continue;
    }
    const list = /^\s{0,3}([-+*]|\d+[.)])\s+(.*)$/.exec(line);
    if (list) {
      const ordered = /^\d/.test(list[1]); const entries: ReactNode[] = [];
      while (index < lines.length) {
        const entry = /^\s{0,3}([-+*]|\d+[.)])\s+(.*)$/.exec(lines[index]);
        if (!entry || /^\d/.test(entry[1]) !== ordered) break;
        const task = /^\[([ xX])\]\s+(.*)$/.exec(entry[2]);
        entries.push(<li key={index}>{task ? <><input type="checkbox" checked={task[1].toLowerCase() === 'x'} readOnly aria-label={task[2]} /> {inline(task[2], tab)}</> : inline(entry[2], tab)}</li>); index++;
      }
      output.push(ordered ? <ol key={key} start={parseInt(list[1], 10)}>{entries}</ol> : <ul key={key}>{entries}</ul>); continue;
    }
    const paragraph = [line]; index++;
    while (index < lines.length && lines[index].trim() && !/^\s{0,3}(#{1,6}\s|>|```|~~~|[-+*]\s|\d+[.)]\s)/.test(lines[index])) {
      if (index + 1 < lines.length && /^\s*(={3,}|-{3,})\s*$/.test(lines[index + 1])) break;
      paragraph.push(lines[index++]);
    }
    output.push(<p key={key}>{inline(paragraph.join('\n'), tab)}</p>);
  }
  return output;
}

export const MarkdownPreview = memo(function MarkdownPreview({ text, tab }: { text: string; tab: FileWorkspaceTab }) {
  const { t } = useTranslation();
  const value = useDeferredValue(text);
  const content = useMemo(() => blocks(value.slice(0, PREVIEW_CHAR_LIMIT), tab), [value, tab.path, tab.sessionId, tab.source]);
  return <article className="vibe-markdown" aria-label={t('markdown.preview')} data-vibe-surface="markdown">
    {content}{text.length > PREVIEW_CHAR_LIMIT && <p role="note">{t('markdown.large')}</p>}
  </article>;
});
