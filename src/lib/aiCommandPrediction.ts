export type AiPredictionProvider = 'openai' | 'claude';

export interface AiPredictionSettings {
  enabled: boolean;
  provider: AiPredictionProvider;
  apiKey: string;
  baseUrl: string;
  model: string;
  debounceMs: number;
  maxTokens: number;
  minChars: number;
}

export interface CommandPredictionRequest {
  input: string;
  history: string[];
  localSuggestions: string[];
}

interface OpenAiChatResponse {
  choices?: Array<{
    message?: {
      content?: string | null;
    };
    text?: string | null;
  }>;
}

interface ClaudeMessageResponse {
  content?: Array<{
    type?: string;
    text?: string;
  }>;
}

export const OPENAI_BASE_URL = 'https://api.openai.com/v1';
export const CLAUDE_BASE_URL = 'https://api.anthropic.com/v1';

export const defaultAiPredictionSettings: AiPredictionSettings = {
  enabled: false,
  provider: 'openai',
  apiKey: '',
  baseUrl: OPENAI_BASE_URL,
  model: 'gpt-4o-mini',
  debounceMs: 450,
  maxTokens: 32,
  minChars: 2,
};

export function getDefaultAiBaseUrl(provider: AiPredictionProvider): string {
  return provider === 'claude' ? CLAUDE_BASE_URL : OPENAI_BASE_URL;
}

export function getDefaultAiModel(provider: AiPredictionProvider): string {
  return provider === 'claude' ? 'claude-3-5-haiku-latest' : 'gpt-4o-mini';
}

export function normalizeAiPredictionSettings(value: unknown): AiPredictionSettings {
  const raw = value && typeof value === 'object' ? value as Partial<AiPredictionSettings> : {};
  const provider: AiPredictionProvider = raw.provider === 'claude' ? 'claude' : 'openai';

  return {
    enabled: typeof raw.enabled === 'boolean' ? raw.enabled : defaultAiPredictionSettings.enabled,
    provider,
    apiKey: typeof raw.apiKey === 'string' ? raw.apiKey : '',
    baseUrl: typeof raw.baseUrl === 'string' && raw.baseUrl.trim()
      ? raw.baseUrl.trim()
      : getDefaultAiBaseUrl(provider),
    model: typeof raw.model === 'string' && raw.model.trim()
      ? raw.model.trim()
      : getDefaultAiModel(provider),
    debounceMs: clampNumber(raw.debounceMs, 150, 2000, defaultAiPredictionSettings.debounceMs),
    maxTokens: clampNumber(raw.maxTokens, 8, 128, defaultAiPredictionSettings.maxTokens),
    minChars: clampNumber(raw.minChars, 1, 20, defaultAiPredictionSettings.minChars),
  };
}

export function isAiPredictionReady(settings: AiPredictionSettings): boolean {
  return Boolean(
    settings.enabled &&
    settings.apiKey.trim() &&
    settings.baseUrl.trim() &&
    settings.model.trim()
  );
}

export async function predictCommandSuffix(
  settings: AiPredictionSettings,
  request: CommandPredictionRequest,
  signal?: AbortSignal
): Promise<string> {
  const normalizedSettings = normalizeAiPredictionSettings(settings);
  const trimmedInput = request.input.trim();

  if (!isAiPredictionReady(normalizedSettings) || trimmedInput.length < normalizedSettings.minChars) {
    return '';
  }

  const prompt = buildPredictionPrompt(request);
  const rawPrediction = normalizedSettings.provider === 'claude'
    ? await predictWithClaude(normalizedSettings, prompt, signal)
    : await predictWithOpenAi(normalizedSettings, prompt, signal);

  return normalizePredictionToSuffix(request.input, rawPrediction);
}

function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  if (typeof value !== 'number' || Number.isNaN(value)) {
    return fallback;
  }

  return Math.min(max, Math.max(min, Math.round(value)));
}

function trimBaseUrl(baseUrl: string): string {
  return baseUrl.replace(/\/+$/, '');
}

function buildPredictionPrompt(request: CommandPredictionRequest): string {
  const history = request.history
    .slice(-12)
    .map((command) => `- ${command}`)
    .join('\n') || '- none';
  const localSuggestions = request.localSuggestions
    .slice(0, 8)
    .map((suggestion) => `- ${suggestion}`)
    .join('\n') || '- none';

  return [
    'You are VibeShell command prediction.',
    'Return only the text that should be appended after CURRENT_INPUT.',
    'Do not include a shell prompt, Markdown, quotes, commentary, or a newline.',
    'If the next characters are not obvious, return an empty string.',
    '',
    `CURRENT_INPUT: ${JSON.stringify(request.input)}`,
    'RECENT_HISTORY:',
    history,
    'LOCAL_CANDIDATES:',
    localSuggestions,
  ].join('\n');
}

async function predictWithOpenAi(
  settings: AiPredictionSettings,
  prompt: string,
  signal?: AbortSignal
): Promise<string> {
  const response = await fetch(`${trimBaseUrl(settings.baseUrl)}/chat/completions`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${settings.apiKey.trim()}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      model: settings.model,
      messages: [
        {
          role: 'system',
          content: 'You complete shell commands. Return only the suffix to append.',
        },
        {
          role: 'user',
          content: prompt,
        },
      ],
      temperature: 0,
      max_tokens: settings.maxTokens,
      stop: ['\n', '\r'],
    }),
    signal,
  });

  if (!response.ok) {
    throw new Error(`OpenAI-compatible prediction failed: ${response.status}`);
  }

  const data = await response.json() as OpenAiChatResponse;
  return data.choices?.[0]?.message?.content ?? data.choices?.[0]?.text ?? '';
}

async function predictWithClaude(
  settings: AiPredictionSettings,
  prompt: string,
  signal?: AbortSignal
): Promise<string> {
  const response = await fetch(`${trimBaseUrl(settings.baseUrl)}/messages`, {
    method: 'POST',
    headers: {
      'x-api-key': settings.apiKey.trim(),
      'anthropic-version': '2023-06-01',
      'anthropic-dangerous-direct-browser-access': 'true',
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      model: settings.model,
      max_tokens: settings.maxTokens,
      temperature: 0,
      system: 'You complete shell commands. Return only the suffix to append.',
      messages: [
        {
          role: 'user',
          content: prompt,
        },
      ],
      stop_sequences: ['\n', '\r'],
    }),
    signal,
  });

  if (!response.ok) {
    throw new Error(`Claude-compatible prediction failed: ${response.status}`);
  }

  const data = await response.json() as ClaudeMessageResponse;
  return data.content?.find((part) => part.type === 'text' && part.text)?.text ?? '';
}

function cleanPredictionText(raw: string): string {
  let text = raw.trim();

  if (!text) {
    return '';
  }

  if (text.startsWith('{') && text.endsWith('}')) {
    try {
      const parsed = JSON.parse(text) as Record<string, unknown>;
      const value = parsed.suffix ?? parsed.completion ?? parsed.command;
      if (typeof value === 'string') {
        text = value.trim();
      }
    } catch {
      // Fall through to plain-text cleanup.
    }
  }

  text = text
    .replace(/^```(?:\w+)?\s*/i, '')
    .replace(/\s*```$/i, '')
    .trim()
    .replace(/^\$\s*/, '')
    .replace(/^["'`]|["'`]$/g, '');

  const firstLine = text.split(/\r?\n/)[0] ?? '';
  return firstLine.replace(/[\x00-\x08\x0B-\x1F\x7F]/g, '').trimEnd();
}

function startsWithIgnoreCase(value: string, prefix: string): boolean {
  return value.toLowerCase().startsWith(prefix.toLowerCase());
}

function getCurrentToken(input: string): string {
  const match = input.match(/(?:^|[\s;&|])([^\s;&|]*)$/);
  return match?.[1] ?? '';
}

export function normalizePredictionToSuffix(input: string, rawPrediction: string): string {
  const prediction = cleanPredictionText(rawPrediction);

  if (!prediction) {
    return '';
  }

  if (startsWithIgnoreCase(prediction, input)) {
    return prediction.slice(input.length);
  }

  const trimmedInput = input.trimStart();
  if (trimmedInput && startsWithIgnoreCase(prediction, trimmedInput)) {
    return prediction.slice(trimmedInput.length);
  }

  const inputWithoutTrailingSpace = input.trimEnd();
  if (
    input.endsWith(' ') &&
    inputWithoutTrailingSpace &&
    startsWithIgnoreCase(prediction, `${inputWithoutTrailingSpace} `)
  ) {
    return prediction.slice(inputWithoutTrailingSpace.length + 1);
  }

  const currentToken = getCurrentToken(input);
  if (currentToken && startsWithIgnoreCase(prediction, currentToken)) {
    return prediction.slice(currentToken.length);
  }

  if (input.includes(' ') && currentToken) {
    return '';
  }

  return prediction;
}
