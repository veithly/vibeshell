import { useMemo, useRef } from 'react';
import hljs from 'highlight.js/lib/common';
import clojure from 'highlight.js/lib/languages/clojure';
import dart from 'highlight.js/lib/languages/dart';
import elixir from 'highlight.js/lib/languages/elixir';
import erlang from 'highlight.js/lib/languages/erlang';
import fsharp from 'highlight.js/lib/languages/fsharp';
import haskell from 'highlight.js/lib/languages/haskell';
import ocaml from 'highlight.js/lib/languages/ocaml';
import scala from 'highlight.js/lib/languages/scala';
import { cn } from '../../lib/utils';

hljs.registerLanguage('clojure', clojure);
hljs.registerLanguage('dart', dart);
hljs.registerLanguage('elixir', elixir);
hljs.registerLanguage('erlang', erlang);
hljs.registerLanguage('fsharp', fsharp);
hljs.registerLanguage('haskell', haskell);
hljs.registerLanguage('ocaml', ocaml);
hljs.registerLanguage('scala', scala);

interface CodeEditorProps {
  value: string;
  language: string;
  readOnly?: boolean;
  onChange: (value: string) => void;
}

const MAX_HIGHLIGHT_CHARACTERS = 500_000;

function highlight(value: string, language: string): string {
  if (value.length > MAX_HIGHLIGHT_CHARACTERS || language === 'text') {
    return value
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  }

  if (language !== 'text' && hljs.getLanguage(language)) {
    return hljs.highlight(value, { language, ignoreIllegals: true }).value;
  }
  return hljs.highlightAuto(value).value;
}

export function CodeEditor({ value, language, readOnly = false, onChange }: CodeEditorProps) {
  const highlightRef = useRef<HTMLElement>(null);
  const highlighted = useMemo(() => `${highlight(value, language)}\n`, [language, value]);

  return (
    <div className="file-code-editor relative h-full min-h-0 overflow-hidden bg-tokyo-bg">
      <pre
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 m-0 overflow-hidden p-4 font-mono text-[13px] leading-6 text-tokyo-fg"
      >
        <code
          ref={highlightRef}
          className="block min-h-full w-max min-w-full whitespace-pre"
          dangerouslySetInnerHTML={{ __html: highlighted }}
        />
      </pre>
      <textarea
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onScroll={(event) => {
          const highlightedCode = highlightRef.current;
          if (!highlightedCode) return;
          highlightedCode.parentElement!.scrollTop = event.currentTarget.scrollTop;
          highlightedCode.parentElement!.scrollLeft = event.currentTarget.scrollLeft;
        }}
        readOnly={readOnly}
        spellCheck={false}
        wrap="off"
        autoCapitalize="off"
        autoCorrect="off"
        aria-label="File editor"
        className={cn(
          'file-code-editor-input absolute inset-0 h-full w-full resize-none overflow-auto border-0 bg-transparent p-4',
          'font-mono text-[13px] leading-6 outline-none',
          'focus:ring-1 focus:ring-inset focus:ring-tokyo-blue',
          readOnly && 'cursor-default'
        )}
      />
    </div>
  );
}

export type { CodeEditorProps };
