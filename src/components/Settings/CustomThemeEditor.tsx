import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Code2, ImagePlus, FileUp, FileDown, Eye, RotateCcw, ShieldOff } from 'lucide-react';
import { CUSTOM_CSS_LIMIT, CUSTOM_THEME_EXAMPLE, useCustomThemeStore, wallpaperCss } from '../../lib/customTheme';
import { safeInvoke } from '../../lib/tauri';

export function CustomThemeEditor() {
  const { t } = useTranslation();
  const theme = useCustomThemeStore();
  const [draft, setDraft] = useState(theme.css);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const id = useId();
  const cssFile = useRef<HTMLInputElement>(null);
  const imageFile = useRef<HTMLInputElement>(null);
  useEffect(() => { setDraft(theme.css); }, [theme.css]);
  useEffect(() => () => useCustomThemeStore.getState().preview(null), []);
  const action = (operation: () => void) => {
    try { setError(''); operation(); } catch (reason) { setError(String(reason)); }
  };
  const importFile = async (file: File | undefined, image: boolean) => {
    if (!file) return;
    try {
      setError('');
      if (file.size > (image ? 1024 * 1024 : CUSTOM_CSS_LIMIT)) throw new Error(t('customTheme.tooLarge'));
      if (!image) { setDraft(await file.text()); return; }
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader(); reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error); reader.readAsDataURL(file);
      });
      const snippet = wallpaperCss(dataUrl);
      setDraft(current => current + snippet);
    } catch (reason) { setError(String(reason)); }
  };
  const exportCss = async () => {
    setBusy(true); setError('');
    try {
      const result = await safeInvoke('export_theme_css', { css: draft });
      if (!result.success) throw new Error(result.error.message);
    } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  };
  return <section className="py-5 space-y-3" aria-labelledby={`${id}-title`} data-vibe-surface="theme-editor">
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className="flex items-center gap-2"><Code2 className="h-4 w-4 text-tokyo-cyan" /><h3 id={`${id}-title`} className="text-sm font-semibold">{t('customTheme.title')}</h3></div>
      <span role="status" className="text-xs text-tokyo-comment">{t(theme.previewCss !== null ? 'customTheme.previewing' : theme.enabled ? 'customTheme.enabled' : 'customTheme.disabled')}</span>
    </div>
    <p className="max-w-prose text-xs leading-relaxed text-tokyo-comment">{t('customTheme.description')}</p>
    <label htmlFor={`${id}-css`} className="sr-only">{t('customTheme.cssLabel')}</label>
    <textarea id={`${id}-css`} value={draft} onChange={event => setDraft(event.target.value)} spellCheck={false}
      className="block w-full min-h-64 resize-y rounded-md border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-6 text-tokyo-fg focus:outline-none focus:ring-1 focus:ring-tokyo-cyan"
      placeholder={CUSTOM_THEME_EXAMPLE} maxLength={CUSTOM_CSS_LIMIT} />
    <div className="flex flex-wrap items-center gap-2">
      <button type="button" className="workspace-action is-active" onClick={() => action(() => theme.save(draft, true))}>{t('customTheme.apply')}</button>
      <button type="button" className="workspace-action" onClick={() => action(() => theme.preview(draft))}><Eye className="h-4 w-4" />{t('customTheme.preview')}</button>
      <button type="button" className="workspace-action" onClick={() => action(() => { theme.preview(null); setDraft(theme.css); })}><RotateCcw className="h-4 w-4" />{t('customTheme.revert')}</button>
      <button type="button" className="workspace-action" onClick={() => theme.disable()}><ShieldOff className="h-4 w-4" />{t('customTheme.disable')}</button>
    </div>
    <div className="flex flex-wrap items-center gap-2">
      <button type="button" className="workspace-action" onClick={() => cssFile.current?.click()}><FileUp className="h-4 w-4" />{t('customTheme.import')}</button>
      <button type="button" className="workspace-action" disabled={busy} onClick={() => void exportCss()}><FileDown className="h-4 w-4" />{t('customTheme.export')}</button>
      <button type="button" className="workspace-action" onClick={() => imageFile.current?.click()}><ImagePlus className="h-4 w-4" />{t('customTheme.image')}</button>
      <button type="button" className="workspace-action" onClick={() => setDraft(current => current + '\n' + CUSTOM_THEME_EXAMPLE)}>{t('customTheme.example')}</button>
      <input ref={cssFile} type="file" accept=".css,text/css" className="hidden" onChange={event => { void importFile(event.target.files?.[0], false); event.target.value = ''; }} />
      <input ref={imageFile} type="file" accept="image/png,image/jpeg,image/webp,image/gif,image/avif,image/bmp,image/svg+xml" className="hidden" onChange={event => { void importFile(event.target.files?.[0], true); event.target.value = ''; }} />
    </div>
    <p className="max-w-prose text-xs leading-relaxed text-tokyo-comment">{t('customTheme.warning')}</p>
    <p className="text-xs text-tokyo-fg">{t('customTheme.recovery')} <kbd className="font-mono">⌘/Ctrl+Shift+F12</kbd></p>
    {error && <p role="alert" className="text-xs text-tokyo-red break-words">{error}</p>}
  </section>;
}
