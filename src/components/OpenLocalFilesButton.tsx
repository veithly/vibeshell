import { useState } from 'react';
import { FolderOpen, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { openLocalFiles } from '../lib/localFiles';
import { useRuntimeCapabilitiesStore } from '../stores/runtimeCapabilitiesStore';

export function OpenLocalFilesButton() {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const capabilities = useRuntimeCapabilitiesStore(state => state.capabilities);
  if (capabilities.isMobile) return null;
  return <button type="button" className="icon-button h-8 w-8 shrink-0" disabled={busy}
    aria-label={t('localFiles.open')} title={`${t('localFiles.open')} (⌘/Ctrl+O)`}
    onClick={() => { setBusy(true); void openLocalFiles().finally(() => setBusy(false)); }}>
    {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <FolderOpen className="h-4 w-4" />}
  </button>;
}
