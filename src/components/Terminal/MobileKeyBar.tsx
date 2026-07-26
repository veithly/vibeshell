import { ClipboardPaste } from 'lucide-react';

interface MobileKeyBarProps {
  onSend: (data: string) => void;
  onPaste: () => void;
}

interface TerminalKey {
  label: string;
  shortLabel?: string;
  value: string;
}

const terminalKeys: readonly TerminalKey[] = [
  { label: 'Esc', value: '\x1b' },
  { label: 'Tab', value: '\t' },
  { label: 'Ctrl-C', shortLabel: '^C', value: '\x03' },
  { label: 'Ctrl-D', shortLabel: '^D', value: '\x04' },
  { label: 'Up', shortLabel: '\u2191', value: '\x1b[A' },
  { label: 'Down', shortLabel: '\u2193', value: '\x1b[B' },
  { label: 'Left', shortLabel: '\u2190', value: '\x1b[D' },
  { label: 'Right', shortLabel: '\u2192', value: '\x1b[C' },
];

export function MobileKeyBar({ onSend, onPaste }: MobileKeyBarProps) {
  return (
    <div className="mobile-terminal-keys" role="toolbar" aria-label="Terminal keys">
      <div className="mobile-terminal-keys-scroll">
        {terminalKeys.map((key) => (
          <button
            key={key.label}
            type="button"
            className="mobile-terminal-key"
            aria-label={key.label}
            onPointerDown={(event) => event.preventDefault()}
            onClick={() => onSend(key.value)}
          >
            {key.shortLabel ?? key.label}
          </button>
        ))}
        <button
          type="button"
          className="mobile-terminal-key"
          aria-label="Paste"
          onPointerDown={(event) => event.preventDefault()}
          onClick={onPaste}
        >
          <ClipboardPaste className="h-4 w-4" aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}
