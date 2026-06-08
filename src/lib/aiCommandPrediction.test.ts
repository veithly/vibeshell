import { describe, expect, it } from 'vitest';
import { normalizePredictionToSuffix } from './aiCommandPrediction';

describe('AI command prediction cleanup', () => {
  it('turns a full command prediction into the suffix to append', () => {
    expect(normalizePredictionToSuffix('git ch', 'git checkout')).toBe('eckout');
  });

  it('accepts suffix-only predictions', () => {
    expect(normalizePredictionToSuffix('docker ', 'ps --format json')).toBe('ps --format json');
  });

  it('drops unrelated full-command predictions in argument position', () => {
    expect(normalizePredictionToSuffix('git ch', 'docker ps')).toBe('');
  });
});
